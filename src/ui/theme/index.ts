import { effect, setColorScheme, signal } from "nsh";
import { Core } from "haiku/native";
import base from "./haiku.json";

export type AppearanceMode = "system" | "light" | "dark";
export const brands = [
  ["red", "红"], ["orange", "橙"], ["amber", "琥珀"], ["yellow", "黄"],
  ["lime", "青柠"], ["green", "绿"], ["emerald", "翡翠"], ["teal", "青绿"],
  ["cyan", "青"], ["sky", "天蓝"], ["blue", "蓝"], ["indigo", "靛蓝"],
  ["violet", "紫罗兰"], ["purple", "紫"], ["fuchsia", "品红"],
  ["pink", "粉"], ["rose", "玫瑰"],
] as const;
export type Brand = typeof brands[number][0];
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
export function brandTheme(brand: Brand) {
  const original = base.cssVars.light[`brand-${brand}`];
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
      "sidebar-ring": primary, "progress-played": primary,
    };
  }
  return { cssVars: { theme: base.cssVars.theme, light: palette(false), dark: palette(true) } };
}
export function createAppearance(initialMode: AppearanceMode = "system") {
  const mode = signal<AppearanceMode>(initialMode);
  const brand = signal<Brand>("orange");
  effect(() => setColorScheme(mode()));
  effect(() => {
    Core.applyTheme(JSON.stringify(brandTheme(brand()))).catch((error) => console.error("Theme update failed", error));
  });
  return { mode, brand };
}
