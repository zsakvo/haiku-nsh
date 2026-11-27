import { Canvas, colorScheme, createAnimatedValue, effect, reduceMotion, resource, signal, token, untracked, View, type AnimatedNode, type CanvasRenderingContext2D, type CanvasSize } from "nsh";
import type { Core } from "haiku/native";
import { navigationBackdrop } from "./Chrome";

/** A static ASCII sky behind the playlist detail header, after unvalley/ascii-sky: the sky
 * is a field of glyphs, clouds are clear gaps with soft glyph edges, all tinted by the cover.
 * It fills the playlist header and scrolls with it: from under the bar it thins and fades out
 * before the track table. On a
 * new cover it unfurls once from the top (natively animated alpha, no JS per frame); otherwise
 * it is drawn once per size/cover/appearance and stays still. Colors are cover data from `Core.coverSky`. */

type Ink = { ink: string; glow: string };
type Sky = { light: Ink; dark: Ink };

const CELL_W = 7, CELL_H = 12;
/** Sparse to dense. */
const RAMP = [" ", ".", ":", "-", "+", "*", "o", "x", "%", "#"];
const FONT = `${CELL_H - 2}px monospace`;
/** Glyph density buckets drawn at increasing opacity, so dense sky reads deeper than cloud edges. */
const LEVELS = [[1, 2, 3], [4, 5, 6], [7, 8, 9]];

function hash(seed: number, x: number, y: number) {
  let h = (seed ^ Math.imul(x, 0x27d4eb2d) ^ Math.imul(y, 0x165667b1)) >>> 0;
  h = Math.imul(h ^ (h >>> 15), 0x85ebca6b);
  h = Math.imul(h ^ (h >>> 13), 0xc2b2ae35);
  return ((h ^ (h >>> 16)) >>> 0) / 4294967296;
}
const smooth = (t: number) => t * t * (3 - 2 * t);
const clamp01 = (t: number) => Math.min(1, Math.max(0, t));
const ramp = (from: number, to: number, t: number) => smooth(clamp01((t - from) / (to - from)));

function noise(seed: number, x: number, y: number) {
  const x0 = Math.floor(x), y0 = Math.floor(y), fx = smooth(x - x0), fy = smooth(y - y0);
  const top = hash(seed, x0, y0) * (1 - fx) + hash(seed, x0 + 1, y0) * fx;
  const bottom = hash(seed, x0, y0 + 1) * (1 - fx) + hash(seed, x0 + 1, y0 + 1) * fx;
  return top * (1 - fy) + bottom * fy;
}
function fbm(seed: number, x: number, y: number) {
  let sum = 0, amp = 0.5, norm = 0;
  for (let o = 0; o < 4; o++) {
    const f = 2 ** o;
    sum += noise(seed + o * 101, x * f, y * f) * amp;
    norm += amp; amp *= 0.5;
  }
  return sum / norm;
}

function seedOf(text: string) {
  let h = 2166136261;
  for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 16777619);
  return h >>> 0;
}
function rgba(hex: string, alpha: number) {
  const n = parseInt(hex.slice(1), 16);
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${alpha})`;
}

/** One-shot unfurl from the top; rows start in turn over the first part of it. */
const REVEAL_MS = 1600, REVEAL_SPREAD = 0.7;

/** Glyph index per cell: dense sky, clear clouds with soft edges, thinning toward the bottom. */
function skyGrid(seed: number, cols: number, rows: number, advance: number) {
  const grid: number[][] = [];
  for (let row = 0; row < rows; row++) {
    const fade = 1 - ramp(0.3, 1, row / rows);
    const line: number[] = [];
    for (let col = 0; col < cols; col++) {
      // Cells are taller than wide; stretch rows so clouds keep a flat, natural shape.
      const u = (col * advance) / (CELL_W * 22), v = (row * CELL_H) / (CELL_W * 22 * 1.8);
      const cloud = ramp(0.5, 0.66, fbm(seed, u, v));
      const grain = fbm(seed ^ 0x9e3779b9, col / 3, row / 2) - 0.5;
      const d = clamp01((0.78 + grain * 0.5) * (1 - cloud)) * fade;
      line.push(Math.round(d * (RAMP.length - 1)));
    }
    grid.push(line);
  }
  return grid;
}

/** Built once per cover and size, shared by the sky and its copy under the bar. */
let cached: { key: string; rows: number[][] } | undefined;
function grid(cover: string, w: number, h: number, advance: number) {
  const cols = Math.ceil(w / advance), rows = Math.ceil(h / CELL_H);
  const key = `${cover}|${cols}|${rows}`;
  if (cached?.key !== key) cached = { key, rows: skyGrid(seedOf(cover), cols, rows, advance) };
  return cached.rows;
}

/** Paints the sky; `shown` maps a row's reveal start to its 0..1 visibility (a number or an
 * animated node), and is 1 everywhere once revealed. */
function paint(cx: CanvasRenderingContext2D, size: CanvasSize, cover: string, tint: Ink, dark: boolean,
  shown: (start: number) => number | AnimatedNode, wash: number | AnimatedNode) {
  const gradient = cx.createLinearGradient(0, 0, 0, size.height);
  gradient.addColorStop(0, rgba(tint.glow, dark ? 0.3 : 0.35));
  gradient.addColorStop(0.6, rgba(tint.glow, 0));
  cx.globalAlpha = wash;
  cx.fillStyle = gradient;
  cx.fillRect(0, 0, size.width, size.height);
  cx.font = FONT;
  cx.textBaseline = "top";
  cx.fillStyle = tint.ink;
  // The monospace advance is not CELL_W; measure it so the field reaches the right edge.
  const advance = cx.measureText("x").width || CELL_W;
  const rows = grid(cover, size.width, size.height, advance);
  const levels = dark ? [0.05, 0.09, 0.13] : [0.06, 0.1, 0.14];
  rows.forEach((cells, row) => {
    const t = row / rows.length;
    // Fades all the way out at the bottom.
    const fade = (1 - ramp(0.2, 1, t)) ** 1.4;
    if (fade < 0.01) return;
    const visible = shown(t * REVEAL_SPREAD);
    LEVELS.forEach((level, i) => {
      let line = "", any = false;
      for (const g of cells) {
        if (level.includes(g)) { line += RAMP[g]; any = true; } else line += " ";
      }
      if (!any) return;
      cx.globalAlpha = typeof visible === "number" ? visible * levels[i] * fade : visible.mul(levels[i] * fade);
      cx.fillText(line, 0, row * CELL_H);
    });
  });
}

/** Place it first in the playlist's NavigationScrollHeader; it fills the header edge to edge
 * (past the content insets) and from under the bar, so it scrolls away with the header. */
export function DetailSky(props: { core: Core; active: boolean; cover?: string; source?: string }) {
  const width = signal(0);
  const sky = resource(() => props.cover ? [props.source ?? "", props.cover] as const : undefined,
    async ([source, cover]) => await props.core.coverSky(source, cover) as Sky | null);
  const dark = () => colorScheme() === "dark";
  const ink = () => sky() ? (dark() ? sky()!.dark : sky()!.light) : null;
  // The bar is clear at the top so the sky shows behind it; its glass follows the scroll.
  navigationBackdrop(() => props.active && !!ink());

  // Unfurl once per cover; a redraw for appearance or size keeps it fully shown.
  const progress = createAnimatedValue(0);
  let revealed: string | undefined;
  effect(() => {
    const cover = props.cover, ready = !!ink() && width() > 0;
    untracked(() => {
      if (!ready || revealed === cover) return;
      revealed = cover;
      progress.set(0);
      if (reduceMotion()) progress.set(1);
      else void progress.timing(1, { duration: REVEAL_MS, easing: "easeOut" });
    });
  });
  return <View testID="detail-sky" class="absolute top-0 bottom-0 overflow-hidden pointer-events-none"
    style={{ left: -token("content-inset-left"), right: -token("content-inset-right") }}
    onLayout={(e) => width.set(e.bounds.size.width)}>
    <If when={!!ink() && width() > 0}>
      <Canvas class="absolute inset-0 pointer-events-none" draw={(cx, size) => paint(cx, size, props.cover ?? "", ink()!, dark(),
        (start) => progress.interpolate({ inputRange: [start, start + (1 - REVEAL_SPREAD)], outputRange: [0, 1] }), progress)} />
    </If>
  </View>;
}
