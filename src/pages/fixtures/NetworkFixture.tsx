import { resource, signal, Text, View } from "nsh";
import { Core, PluginService } from "haiku/native";
import { AppShell } from "../../shell/AppShell";
import type { EntityRef, Route } from "../../shell/navigation";
import { Button } from "../../ui/Button";

/** Explicit, isolated loopback source for Headless and read-only native screenshots. */
export function NetworkFixture(props: { page?: "library" | "album" | "artist" | "dock" | "statistics"; controls?: boolean }) {
  const status = signal("");
  const ready = resource(async () => {
    const core = await Core.openIsolated();
    const fixture = await core.sources.startNetworkFixture() as {sourceId:string;url:string};
    const album = PluginService.reference(fixture.sourceId,"album","album/潮汐") as EntityRef;
    const artist = PluginService.reference(fixture.sourceId,"artist","artist/海") as EntityRef;
    const detail = await core.sources.detail(album,null) as any;
    await core.player.setVolume(props.controls ? 0 : 0.15);
    if (props.page === "dock") {
      await core.player.play(detail.tracks.items,0);
      core.collections.setLiked(detail.tracks.items[0],true);
    }
    if (props.page === "statistics") await core.history.seedFixture(detail.tracks.items);
    console.log(`M4-01 isolated loopback fixture ready: ${props.page ?? "library"} source=${fixture.sourceId}; quality=high`);
    const route: Route = props.page === "album" || props.page === "dock" ? {kind:"album",ref:album}
      : props.page === "artist" ? {kind:"artist",ref:artist}
      : props.page === "statistics" ? {kind:"statistics"} : {kind:"source",source:fixture.sourceId};
    return {core,fixture,route};
  });
  return <View class="flex-1">
    <If when={!!ready()}>
      <AppShell initialMode="light" initialRoute={ready()!.route} />
      <If when={props.controls}><View class="flex-row gap-2">
        <Button label="注入一次来源错误" onPress={() => void ready()!.core.sources.invoke(ready()!.fixture.sourceId,"track.get",["fail"],{})
          .then(() => status.set("来源错误已注入")).catch((e) => status.set(String(e)))} />
        <Text>{status()}</Text>
      </View></If>
    </If>
    <If when={!!ready.error()}><Text>{String(ready.error())}</Text></If>
  </View>;
}
