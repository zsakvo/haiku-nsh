/**
 * Brand-driven palette generator. Every neutral surface, border and text color is derived in OKLCH from the
 * brand seed's hue, so the whole window leans toward the chosen brand (Linear's base/accent/contrast model,
 * Radix's tinted grays) instead of a fixed warm gray with only an accent swap. Pure module, no NSH imports.
 */
type Lch = [l: number, c: number, h: number];

const toLinear = (n: number) => n <= 0.04045 ? n / 12.92 : ((n + 0.055) / 1.055) ** 2.4;
const toGamma = (n: number) => n <= 0.0031308 ? n * 12.92 : 1.055 * n ** (1 / 2.4) - 0.055;

export function hexToOklch(hex: string): Lch {
  const [r, g, b] = [1, 3, 5].map((i) => toLinear(parseInt(hex.slice(i, i + 2), 16) / 255));
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  const L = 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s;
  const A = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s;
  const B = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;
  return [L, Math.hypot(A, B), (Math.atan2(B, A) * 180 / Math.PI + 360) % 360];
}

function oklchToRgb([L, C, H]: Lch): [number, number, number] {
  const a = C * Math.cos(H * Math.PI / 180), b = C * Math.sin(H * Math.PI / 180);
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ];
}

const inGamut = (rgb: number[]) => rgb.every((n) => n >= -1e-4 && n <= 1 + 1e-4);

/** OKLCH to sRGB hex; out-of-gamut colors keep lightness and hue and lose chroma. */
export function oklch(l: number, c: number, h: number, alpha?: number): string {
  let lo = 0, hi = c, rgb = oklchToRgb([l, c, h]);
  if (!inGamut(rgb)) {
    for (let i = 0; i < 24; i++) {
      const mid = (lo + hi) / 2;
      if (inGamut(oklchToRgb([l, mid, h]))) lo = mid; else hi = mid;
    }
    rgb = oklchToRgb([l, lo, h]);
  }
  const hex = (n: number) => Math.round(Math.min(1, Math.max(0, n)) * 255).toString(16).padStart(2, "0");
  const out = "#" + rgb.map((n) => hex(toGamma(Math.min(1, Math.max(0, n))))).join("");
  return alpha === undefined ? out : out + Math.round(alpha * 255).toString(16).padStart(2, "0");
}

function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5].map((i) => toLinear(parseInt(hex.slice(i, i + 2), 16) / 255));
  return r * 0.2126 + g * 0.7152 + b * 0.0722;
}

/** White on saturated mid-tones (the perceptual pick, not the strict WCAG crossover); black only on light fills. */
export function onColor(color: string): string {
  return luminance(color) > 0.36 ? "#000000" : "#ffffff";
}

/**
 * Semantic roles for one appearance. `seed` is the brand swatch; only its hue and chroma are used, so every
 * brand gets the same lightness structure and contrast.
 */
export function brandPalette(seed: string, dark: boolean): Record<string, string> {
  const [, seedC, h] = hexToOklch(seed);
  // An achromatic seed selects the plain theme: pure grays, white canvas, ink primary.
  const plain = seedC < 0.02;
  const vivid = plain ? 0 : Math.min(seedC, 0.2);
  // Neutral chroma: enough to read as the brand's family, low enough to stay a gray.
  const n = plain ? 0 : dark ? 0.014 : 0.01;
  const k = plain ? 0 : 1;
  const g = (l: number, k = 1, alpha?: number) => oklch(l, n * k, h, alpha);
  const tint = (l: number, k: number) => oklch(l, vivid * k, h);

  const primary = plain ? oklch(dark ? 0.92 : 0.24, 0, 0) : dark ? oklch(0.76, vivid * 0.78, h) : oklch(0.58, vivid, h);
  const onPrimary = onColor(primary);
  // Tonal action: a faint brand wash carrying brand-colored text and icons.
  const soft = plain ? (dark ? oklch(0.3, 0, 0) : oklch(0.94, 0, 0))
    : dark ? oklch(0.3, vivid * 0.3, h) : oklch(0.94, vivid * 0.2, h);
  const onSoft = plain ? (dark ? oklch(0.96, 0, 0) : oklch(0.22, 0, 0))
    : dark ? oklch(0.82, vivid * 0.7, h) : oklch(0.5, vivid, h);
  const status = dark
    ? { destructive: oklch(0.72, 0.15, 22), success: oklch(0.78, 0.13, 155), warning: oklch(0.83, 0.12, 82) }
    : { destructive: oklch(0.55, 0.19, 25), success: oklch(0.54, 0.13, 152), warning: oklch(0.56, 0.12, 62) };

  const s = dark ? {
    background: g(0.2), foreground: g(0.96, 0.5), card: g(0.23), popover: g(0.25), sidebar: g(0.215),
    sidebarForeground: g(0.87, 0.7), muted: g(0.25), mutedForeground: g(0.72, 1.2),
    secondary: g(0.28), secondaryForeground: g(0.91, 0.6), accent: g(0.3, 1.2),
    chip: g(0.235), chipActive: g(0.29, 1.3), chipForeground: g(0.77, 1),
    border: g(0.3), divider: g(0.28), input: g(0.36), switchTrack: g(0.4), buffered: g(0.42),
    settingsCard: g(0.23), settingsSelected: g(0.27, 1.3),
    selection: plain ? g(0.32) : tint(0.33, 0.32), selectionForeground: plain ? g(0.96) : tint(0.9, 0.4),
    overlay: oklch(0.1, 0.01 * k, h, 0.55), navAlpha: 0.66, chipAlpha: 0.65,
  } : {
    background: plain ? "#ffffff" : g(0.988, 0.4), foreground: g(0.22, 1.6), card: oklch(1, 0, h), popover: oklch(1, 0, h),
    sidebar: g(plain ? 0.97 : 0.963), sidebarForeground: g(0.38, 1.6), muted: g(0.957), mutedForeground: g(0.53, 2),
    secondary: g(0.948), secondaryForeground: g(0.3, 1.6), accent: g(0.935, 1.4),
    chip: g(0.96), chipActive: g(0.925, 1.6), chipForeground: g(0.47, 1.8),
    border: g(0.91, 1.2), divider: g(0.92, 1.1), input: g(0.87, 1.3), switchTrack: g(0.86, 1.3),
    buffered: g(0.85, 1.3), settingsCard: g(0.965), settingsSelected: g(0.925, 1.6),
    selection: plain ? g(0.93) : tint(0.93, 0.22), selectionForeground: plain ? g(0.22) : tint(0.4, 0.75),
    overlay: oklch(0.2, 0.02 * k, h, 0.35), navAlpha: 0.62, chipAlpha: 0.72,
  };

  return {
    background: s.background, foreground: s.foreground,
    card: s.card, "card-foreground": s.foreground,
    popover: s.popover, "popover-foreground": s.foreground,
    sidebar: s.sidebar, "sidebar-foreground": s.sidebarForeground,
    muted: s.muted, "muted-foreground": s.mutedForeground,
    secondary: s.secondary, "secondary-foreground": s.secondaryForeground,
    accent: s.accent, "accent-foreground": s.foreground,
    "category-chip": s.chip, "category-chip-active": s.chipActive, "category-chip-foreground": s.chipForeground,
    border: s.border, "sidebar-border": s.border, input: s.input, "input-foreground": s.foreground,
    "settings-card": s.settingsCard, "settings-divider": s.divider, "settings-switch-track": s.switchTrack,
    "settings-switch-thumb": dark ? s.foreground : "#ffffff", "settings-sidebar-selected": s.settingsSelected,
    "progress-buffered": s.buffered, "progress-played": s.foreground,
    "navigation-backdrop-tint": s.background + Math.round(s.navAlpha * 255).toString(16),
    "dock-surface": s.background + Math.round(s.navAlpha * 255).toString(16),
    "source-chip-tint": (dark ? s.muted : s.card) + Math.round(s.chipAlpha * 255).toString(16),
    "source-chip-border": oklch(dark ? 1 : 0, 0, 0, dark ? 0.12 : 0.09),
    // Source chip: the tonal wash, translucent over the live blur, with a brand-tinted hairline.
    "source-chip-accent-tint": soft + Math.round(s.chipAlpha * 255).toString(16),
    "source-chip-accent-border": plain ? oklch(dark ? 1 : 0, 0, 0, dark ? 0.12 : 0.09) : oklch(dark ? 0.6 : 0.55, vivid * 0.6, h, dark ? 0.3 : 0.18),
    "source-chip-foreground": onSoft,
    "overlay-backdrop": s.overlay,
    primary, "primary-foreground": onPrimary, ring: primary, "primary-soft": soft, "primary-soft-foreground": onSoft,
    "sidebar-primary": primary, "sidebar-primary-foreground": onPrimary, "sidebar-ring": primary,
    selection: s.selection, "selection-foreground": s.selectionForeground,
    "sidebar-accent": s.selection, "sidebar-accent-foreground": s.selectionForeground,
    destructive: status.destructive, "destructive-foreground": onColor(status.destructive),
    success: status.success, "success-foreground": onColor(status.success),
    warning: status.warning, "warning-foreground": onColor(status.warning),
    // Immersive surfaces stay dark in both appearances but share the brand hue.
    "cover-placeholder": oklch(0.32, 0.025 * k, h),
    "now-playing-background": oklch(0.17, 0.02 * k, h), "now-playing-background-fallback": oklch(0.17, 0.02 * k, h),
    "now-playing-foreground": oklch(0.95, 0.008 * k, h), "now-playing-muted": oklch(0.95, 0.008 * k, h, 0.72),
    "lyric-inactive": oklch(0.95, 0.008 * k, h, 0.72),
    "progress-now-playing-played": oklch(0.3, 0.03 * k, h), "progress-now-playing-buffered": oklch(0.52, 0.035 * k, h),
    "dock-on-dark-foreground": oklch(0.97, 0.006 * k, h), "dock-on-dark-muted": oklch(0.86, 0.012 * k, h),
    "dock-on-dark-surface": oklch(0.25, 0.02 * k, h, 0.16),
  };
}
