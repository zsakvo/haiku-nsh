import { effect, resource, Text, View } from "nsh";
import { Window } from "nsh/desktop";
import { Core, NowPlaying, PlayerDock } from "haiku/native";
import { AppShell } from "../../shell/AppShell";
import { createAppearance } from "../../ui/theme";

function PlaybackScene() {
  createAppearance("dark");
  effect(() => { Core.initialize(); Window.setTitle("Haiku · M6-04 Dark"); });
  return <View class="flex-1 bg-background"><NowPlaying open /><PlayerDock /></View>;
}

/** Real macOS glass acceptance over dark Now Playing or the light application shell. */
export function GlassDockFixture(props: { light?: boolean }) {
  const ready = resource(async () => {
    const core = await Core.openIn("/Users/zsakvo/Project/GPUI/haiku-nsh/.artifacts/m6-04/data");
    const fixture = await core.sources.startNetworkFixture() as { sourceId: string; url: string };
    await core.settings.update({
      themeMode: props.light ? "light" : "dark", lyricsAutoSearch: true,
      lyricsProvidersDisabled: ["betterlyrics", "lrclib", "kugou", "paxsenix", "lyricsplus"],
      lyricsCustomProviders: [{ id: "glass-proof", name: "离线逐字歌词",
        urlTemplate: `${fixture.url}/lyrics`, responseType: "json", jsonPath: "text", headers: {} }],
      desktopLyrics: { visible: false, locked: false, background: "solid", outline: true,
        shadow: false, fontSize: 36, layout: "classic" },
    });
    const track = { source: fixture.sourceId, id: "1", title: "潮汐之间",
      artists: ["林间回声"], duration: 60, cover: `${fixture.url}/cover.png` };
    core.collections.setLiked(track, true);
    await core.player.setVolume(0.08);
    await core.player.play([track], 0);
    await core.player.setMode("repeatOne");
    return core;
  });
  return <View class="flex-1">
    <If when={!!ready()}><If when={props.light} else={<PlaybackScene />}><AppShell initialMode="light" /></If></If>
    <If when={!!ready.error()}><Text>{String(ready.error())}</Text></If>
  </View>;
}

