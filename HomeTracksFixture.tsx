import { cleanup, mount, resource, Scroll, signal, Text, View } from "nsh";
import { Core } from "haiku/native";
import { HomeTracks, type HomeTrack } from "./src/pages/HomeTracks";
import { createAppearance } from "./src/ui/theme";
import { Button } from "./src/ui/Button";

function Fixture() {
  const ready = resource(async () => {
    const core = await Core.openIsolated();
    await core.player.setVolume(0);
    const fixture = await core.sources.startNetworkFixture() as { sourceId: string };
    const feed = await core.sources.home(fixture.sourceId, false) as { shelves: { kind: string; items: HomeTrack[] }[] };
    const original = feed.shelves.find(shelf => shelf.kind === "tracks")!.items;
    const titles = ["樱华彩", "花亦山", "天和山雨雪", "望穿秋水", "芒种", "琵琶曲",
      "夜航", "晨间轻听", "山海", "风起", "落日", "归途", "第十三首", "最后一首"];
    return { core, tracks: titles.map((title, index) => ({
      ...original[index % original.length], id: String(index + 1), title,
      quality: index % 3 === 0 ? ["hires"] : index % 3 === 1 ? ["lossless"] : [],
    })) };
  });
  return <View class="flex-1 bg-background">
    <If when={!!ready()}><Panel core={ready()!.core} tracks={ready()!.tracks} /></If>
    <If when={!!ready.error()}><Text>{String(ready.error())}</Text></If>
  </View>;
}
function Panel(props: { core: Core; tracks: HomeTrack[] }) {
  const appearance = createAppearance("light");
  const count = signal(14), report = signal(""), likes = signal(0);
  cleanup(props.core.player.onState(value => {
    const state = value as { tracks: HomeTrack[]; current?: HomeTrack };
    report.set(`队列 ${state.tracks.length} 首 · ${state.current?.title ?? "空"}`);
  }));
  cleanup(props.core.collections.onLiked(value => likes.set((value as string[]).length)));
  return <Scroll testID="home-tracks-page" class="flex-1" contentClass="pl-(--content-inset-left) pr-(--content-inset-right) py-5 gap-4">
    <Text role="header" class="text-base font-semibold text-foreground">为你推荐的歌曲</Text>
    <HomeTracks core={props.core} tracks={props.tracks.slice(0, count())} active />
    <View class="flex-row gap-2">
      <Button label="只显示两首" onPress={() => count.set(2)} />
      <Button label="恢复全部歌曲" onPress={() => count.set(14)} />
      <Button label="切换深色" onPress={() => void appearance.setMode("dark")} />
      <Button label="切换浅色" onPress={() => void appearance.setMode("light")} />
    </View>
    <Text class="text-foreground">{report()}</Text><Text class="text-foreground">{`喜欢 ${likes()} 首`}</Text>
    <View class="h-96 shrink-0" />
  </Scroll>;
}
mount(Fixture);
