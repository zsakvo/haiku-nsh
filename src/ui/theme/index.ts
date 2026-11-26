import { cleanup, effect, setColorScheme, signal } from "nsh";
import { Core } from "haiku/native";
import base from "./haiku.json";
import { brandPalette } from "./palette";

export type AppearanceMode = "system" | "light" | "dark";
export const brands = [
  ["neutral", "素"], ["blue", "蓝"], ["purple", "紫"], ["pink", "粉"], ["red", "红"],
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
/** Color arithmetic belongs with the theme definitions, never in components. */
export function brandTheme(brand: Brand | string) {
  const lightVars = base.cssVars.light as Record<string, string>;
  const seed = lightVars[`brand-${brand}`] ?? lightVars["brand-orange"];
  const palette = (dark: boolean) => ({ ...(dark ? base.cssVars.dark : base.cssVars.light), ...brandPalette(seed, dark) });
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
