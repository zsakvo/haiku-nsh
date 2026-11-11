import { cleanup, resource, signal, Text, View } from "nsh";
import { Core } from "haiku/native";
import { AppShell } from "../../shell/AppShell";
import { Button } from "../../ui/Button";

/** Offline real-window and Headless fixture, with a fresh data root for every mount. */
export function RemoteLyricsFixture(props: { page?: "playing" | "search" | "settings"; controls?: boolean }) {
  const state = signal<any>({}), status = signal("");
  let stop: (() => void) | undefined;
  cleanup(() => stop?.());
  const ready = resource(async () => {
    const core = await Core.openIsolated();
    const fixture = await core.sources.startNetworkFixture() as { sourceId: string; url: string };
    const disabled = ["betterlyrics", "lrclib", "kugou", "paxsenix", "lyricsplus"];
    await core.settings.update({
      lyricsAutoSearch: true, lyricsProvidersDisabled: disabled,
      lyricsCustomProviders: [
        { id: "fixture", name: "回声歌词", urlTemplate: `${fixture.url}/lyrics?title={title}`,
          responseType: "json", jsonPath: "text", headers: {} },
        { id: "broken", name: "离线来源", urlTemplate: `${fixture.url}/lyrics-error`,
          responseType: "json", jsonPath: "text", headers: {} },
      ],
    });
    await core.player.append([{
      source: fixture.sourceId, id: "1", title: "潮汐之间", artists: ["林间回声"], duration: 60,
      cover: `${fixture.url}/cover.png`, album: { id: "album/潮汐", title: "潮汐与回声" },
    }]);
    state.set(core.lyrics.snapshot());
    stop = core.lyrics.onChange(s => state.set(s));
    return { core, fixture };
  });
  return <View class="flex-1"><If when={!!ready()}>
    <AppShell initialMode="light"
      initialLyricsSearch={props.page === "search"}
      initialOverlay={props.page === "settings" ? "settings" : undefined}
      initialSettingsCategory="歌词" />
    <If when={props.controls}><View class="flex-row items-center gap-2">
      <Text>{state().loading ? "远端加载中" : "远端歌词就绪"}</Text><Text>{status()}</Text>
      <Button label="检查禁用请求数" onPress={() => void (async () => {
        const { core, fixture } = ready()!;
        const before = await (await fetch(`${fixture.url}/lyrics-count`)).text();
        const report = await core.lyrics.searchReport({ title: "潮汐之间", artist: "林间回声" }, ["fixture"]) as any;
        const after = await (await fetch(`${fixture.url}/lyrics-count`)).text();
        status.set(`禁用检查 ${report.candidates.length} 项 ${Number(after) - Number(before)} 次请求`);
      })().catch(e => status.set(String(e)))} />
    </View></If>
  </If><If when={!!ready.error()}><Text>{String(ready.error())}</Text></If></View>;
}
