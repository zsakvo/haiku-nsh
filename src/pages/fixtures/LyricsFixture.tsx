import { cleanup, mount, resource, signal, Text, View } from "nsh";
import { Core } from "haiku/native";
import { Button } from "../../ui/Button";

/** Export/persistence acceptance without public providers or an audio device. */
function LyricsFixture() {
  const state = signal<any>({});
  const status = signal("");
  const error = signal("");
  let current: Core;
  let stop: (() => void) | undefined;
  cleanup(() => stop?.());
  const track = (id: string) => ({
    source: "local", id, title: id, artists: ["Fixture singer"],
    localPath: `/tmp/haiku-m1-03-fixtures/missing-${id}.flac`, duration: 60,
  });
  function attach(core: Core) {
    stop?.();
    current = core;
    state.set(core.lyrics.snapshot());
    stop = core.lyrics.onChange((s) => state.set(s));
  }
  const ready = resource(async () => {
    attach(await Core.openIsolated());
    await current.player.append([track("Alpha")]);
    return true;
  });
  async function act(work: () => Promise<unknown>) {
    try { error.set(""); await work(); } catch (e) { error.set(String(e)); }
  }
  return <View class="flex-1 gap-2">
    <If when={ready()}>
      <Text>{state().loading ? "歌词加载中" : `歌词就绪 ${state().track?.title ?? "空"}`}</Text>
      <Text>{state().lyrics?.lines?.map((line: any) => line.text).join(" / ") || "暂无歌词"}</Text>
      <Text>{error() || status()}</Text>
      <Button label="采用夹具歌词" onPress={() => void act(async () => {
        const candidate = { providerId: "fixture", candidateId: "manual",
          inlineResult: { text: "[00:01]<00:01>手动歌词<00:02>", format: "lrc" } };
        const preview = await current.lyrics.resolve(candidate) as any;
        if (preview.lines[0].words[0].text !== "手动歌词") throw new Error("Preview missing words");
        await current.lyrics.adopt(candidate);
      })} />
      <Button label="重开歌词 Core" onPress={() => void act(async () => {
        await current.close(); attach(await Core.open()); status.set("Core 已重开");
      })} />
      <Button label="切换歌词曲目" onPress={() => void act(async () => {
        await current.player.clear(); await current.player.append([track("Beta")]);
      })} />
      <Button label="返回歌词曲目" onPress={() => void act(async () => {
        await current.player.clear(); await current.player.append([track("Alpha")]);
      })} />
      <Button label="恢复自动歌词" onPress={() => void act(() => current.lyrics.resetToAuto())} />
      <Button label="空来源搜索" onPress={() => void act(async () => {
        const results = await current.lyrics.search({ title: "Alpha", artist: "Fixture singer" }, []);
        status.set(`空来源候选 ${(results as any[]).length}`);
      })} />
    </If>
    <If when={!!ready.error()}><Text>{String(ready.error())}</Text></If>
  </View>;
}
mount(LyricsFixture);
