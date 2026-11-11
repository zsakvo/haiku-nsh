import { cleanup, resource, signal } from "nsh";
import { Core } from "haiku/native";
export interface AudioEffects {
  enabled: boolean; bandGainsDb: number[]; presetId: string;
  customPresets: { id: string; name: string; gains: number[] }[];
  autoPreamp: boolean; preampDb: number; stereoWidth: number; balance: number; mono: boolean;
}
export interface SettingsSnapshot {
  schemaVersion: number; themeMode: "light" | "dark" | "system"; brandColor: string;
  audioEffects: AudioEffects; volumeNormalization: boolean; preventScreenSleepOnPlayer: boolean;
  mediaCacheEnabled: boolean; mediaCacheMaxBytes: number;
  lyricsAutoSearch: boolean; lyricsProviderOrder: string[]; lyricsProvidersDisabled: string[];
  lyricsCustomProviders: CustomLyricsProvider[];
}
export interface CustomLyricsProvider {
  id: string; name: string; urlTemplate: string; responseType: string;
  jsonPath: string; headers: Record<string, string>;
}
export function withLyricsDefaults(settings: SettingsSnapshot): SettingsSnapshot {
  return {
    ...settings,
    lyricsAutoSearch: settings.lyricsAutoSearch ?? true,
    lyricsProviderOrder: settings.lyricsProviderOrder ?? [],
    lyricsProvidersDisabled: settings.lyricsProvidersDisabled ?? ["lyricsplus"],
    lyricsCustomProviders: settings.lyricsCustomProviders ?? [],
  };
}
export function lyricsProviders(settings: SettingsSnapshot) {
  const all = [
    { id: "betterlyrics", name: "Better Lyrics" }, { id: "lrclib", name: "LRCLIB" },
    { id: "kugou", name: "Kugou" }, { id: "paxsenix", name: "Paxsenix" },
    { id: "lyricsplus", name: "LyricsPlus" }, ...settings.lyricsCustomProviders,
  ];
  return all.sort((a, b) => {
    const index = (id: string) => {
      const i = settings.lyricsProviderOrder.indexOf(id);
      return i < 0 ? all.length : i;
    };
    return index(a.id) - index(b.id);
  });
}
/** Rust is the writer; this signal mirrors only durable, applied settings. */
export function createSettings() {
  const value = signal<SettingsSnapshot | null>(null);
  const error = signal("");
  let stop: (() => void) | undefined;
  cleanup(() => stop?.());
  const core = resource(async () => {
    const c = await Core.open();
    value.set(withLyricsDefaults(c.settings.get() as SettingsSnapshot));
    stop = c.settings.onChange((next) => value.set(withLyricsDefaults(next as SettingsSnapshot)));
    return c;
  });
  async function update(patch: unknown) {
    try { const c = core() ?? await Core.open(); await c.settings.update(patch); error.set(""); }
    catch (e) { error.set(`无法保存设置：${String(e)}`); }
  }
  return { value, error, core, update };
}
