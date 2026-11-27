import { Canvas, colorScheme, resource, signal, View } from "nsh";
import type { Core } from "haiku/native";
import { navigationBackdrop } from "./Chrome";

/** A static ASCII sky behind the playlist detail header, after unvalley/ascii-sky: the sky
 * is a field of glyphs, clouds are clear gaps with soft glyph edges, all tinted by the cover.
 * Drawn once per size/cover/appearance (no animation), and faded out toward the track table
 * so the header text stays readable. Colors are cover data from `Core.coverSky`. */

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

/** Glyph index per cell: dense sky, clear clouds with soft edges, thinning toward the bottom. */
function skyGrid(seed: number, cols: number, rows: number, advance: number) {
  const grid: number[][] = [];
  for (let row = 0; row < rows; row++) {
    const fade = 1 - ramp(0.4, 1, row / rows);
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

/** Fills its positioned parent; place it first so header content draws above it. */
export function DetailSky(props: { core: Core; active: boolean; cover?: string; source?: string; insetLeft: number; insetRight: number }) {
  const width = signal(0);
  const sky = resource(() => props.cover ? [props.source ?? "", props.cover] as const : undefined,
    async ([source, cover]) => await props.core.coverSky(source, cover) as Sky | null);
  // The sky starts at the very top, behind the floating bar; the bar stays clear above it at rest.
  navigationBackdrop(() => props.active && !!sky());
  const dark = () => colorScheme() === "dark";
  const ink = () => sky() ? (dark() ? sky()!.dark : sky()!.light) : null;
  // Built once per cover and size; a redraw for appearance alone reuses it.
  let cached: { key: string; rows: number[][] } | undefined;
  const grid = (w: number, h: number, advance: number) => {
    const cols = Math.ceil(w / advance), rows = Math.ceil(h / CELL_H);
    const key = `${props.cover}|${cols}|${rows}`;
    if (cached?.key !== key) cached = { key, rows: skyGrid(seedOf(props.cover ?? ""), cols, rows, advance) };
    return cached.rows;
  };
  const alphas = () => dark() ? [0.1, 0.17, 0.24] : [0.12, 0.2, 0.28];
  return <View testID="detail-sky" class="absolute top-0 bottom-0 overflow-hidden pointer-events-none"
    style={{ left: -props.insetLeft, right: -props.insetRight }}
    onLayout={(e) => width.set(e.bounds.size.width)}>
    <If when={!!ink() && width() > 0}>
      <Canvas class="absolute inset-0 pointer-events-none" draw={(cx, size) => {
        const tint = ink()!;
        const wash = cx.createLinearGradient(0, 0, 0, size.height);
        wash.addColorStop(0, rgba(tint.glow, dark() ? 0.5 : 0.55));
        wash.addColorStop(0.85, rgba(tint.glow, 0));
        cx.fillStyle = wash;
        cx.fillRect(0, 0, size.width, size.height);
        cx.font = FONT;
        cx.textBaseline = "top";
        cx.fillStyle = tint.ink;
        // The monospace advance is not CELL_W; measure it so the field reaches the right edge.
        const advance = cx.measureText("x").width || CELL_W;
        const rows = grid(size.width, size.height, advance), levels = alphas();
        LEVELS.forEach((level, i) => {
          cx.globalAlpha = levels[i];
          rows.forEach((cells, row) => {
            let line = "", any = false;
            for (const g of cells) {
              if (level.includes(g)) { line += RAMP[g]; any = true; } else line += " ";
            }
            if (any) cx.fillText(line, 0, row * CELL_H);
          });
        });
      }} />
    </If>
  </View>;
}
