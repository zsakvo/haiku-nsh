import { cleanup, effect, setColorScheme, signal } from "nsh";
import { Core } from "haiku/native";
import base from "./haiku.json";

export type AppearanceMode = "system" | "light" | "dark";
export const brands = [
  ["blue", "蓝"], ["purple", "紫"], ["pink", "粉"], ["red", "红"],
  ["orange", "橙"], ["amber", "黄"], ["green", "绿"],
] as const;
export type Brand = typeof brands[number][0];
const brandIds = new Set<string>(brands.map(([id]) => id));
function normalizeBrand(value: string): Brand {
  return (brandIds.has(value) ? value : "orange") as Brand;
}
export const modes = [
  { id: "system", label: "跟随系统" }, { id: "light", label: "浅色" }, { id: "dark", label: "深色" },
] as const;
function mix(a: string, b: string, amount: number): string {
  return "#" + [1, 3, 5].map((offset) => Math.round(
    parseInt(a.slice(offset, offset + 2), 16) * (1 - amount) +
    parseInt(b.slice(offset, offset + 2), 16) * amount,
  ).toString(16).padStart(2, "0")).join("");
}
function foreground(color: string): string {
  const [r, g, b] = [1, 3, 5].map((offset) => {
    const n = parseInt(color.slice(offset, offset + 2), 16) / 255;
    return n <= 0.04045 ? n / 12.92 : ((n + 0.055) / 1.055) ** 2.4;
  });
  return r * .2126 + g * .7152 + b * .0722 > .179 ? "#000000" : "#ffffff";
}
/** Color arithmetic belongs with the theme definitions, never in components. */
export function brandTheme(brand: Brand | string) {
  const lightVars = base.cssVars.light as Record<string, string>;
  const original = lightVars[`brand-${brand}`] ?? base.cssVars.light["brand-orange"];
  function palette(dark: boolean) {
    const source = dark ? base.cssVars.dark : base.cssVars.light;
    const primary = dark ? mix(original, "#ffffff", .48) : original;
    const onPrimary = foreground(primary);
    const selection = mix(source.sidebar, original, dark ? .25 : .12);
    return {
      ...source, primary, "primary-foreground": onPrimary, ring: primary,
      "sidebar-primary": primary, "sidebar-primary-foreground": onPrimary,
      selection, "selection-foreground": source.foreground,
      "sidebar-accent": selection, "sidebar-accent-foreground": source.foreground,
      "sidebar-ring": primary, "progress-played": source.foreground,
    };
  }
  return { cssVars: { theme: base.cssVars.theme, light: palette(false), dark: palette(true) } };
}
export function createAppearance(initialMode?: AppearanceMode) {
  const mode = signal<AppearanceMode>(initialMode ?? "light");
  const brand = signal<Brand>("orange");
  const error = signal("");
  let core: Core | undefined;
  let stop: (() => void) | undefined;
  cleanup(() => stop?.());
  function mirror(value: unknown) {
    const s = value as { themeMode: AppearanceMode; brandColor: string };
    mode.set(s.themeMode); brand.set(normalizeBrand(s.brandColor));
  }
  effect(() => { let alive = true;
    Core.open().then(async (c) => { if (!alive) return;
      if (initialMode) await c.settings.update({ themeMode: initialMode });
      if (!alive) return;
      core = c; mirror(c.settings.get());
      stop = c.settings.onChange(mirror);
    }).catch((e) => error.set(String(e)));
    return () => { alive = false; };
  });
  async function update(patch: Partial<{ themeMode: AppearanceMode; brandColor: Brand }>) {
    if (patch.themeMode) mode.set(patch.themeMode);
    if (patch.brandColor) brand.set(patch.brandColor);
    try { const c = core ?? await Core.open(); await c.settings.update(patch); error.set(""); }
    catch (e) { error.set(`无法保存外观：${String(e)}`); }
  }
  effect(() => setColorScheme(mode()));
  effect(() => {
    Core.applyTheme(JSON.stringify(brandTheme(brand()))).catch((error) => console.error("Theme update failed", error));
  });
  return { mode, brand, error, setMode: (themeMode: AppearanceMode) => update({ themeMode }),
    setBrand: (brandColor: Brand) => update({ brandColor }) };
}
