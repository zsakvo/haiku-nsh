import { cleanup, resource, signal, Text, View } from "nsh";
import { Core, LibraryService } from "haiku/native";
import { AppShell } from "../../shell/AppShell";
import type { EntityRef, Route } from "../../shell/navigation";
import { ScrapeReview } from "../ScrapeReview";
import { prepareLibrary, removeLibrary } from "./libraryAudio";

/** Each mount owns an isolated Core and explicit loopback sources. */
export function M405Fixture(props: {
  page?: "favorite" | "offline" | "library" | "review" | "progress" | "settings";
}) {
  let root: string | undefined;
  cleanup(() => { if (root) void removeLibrary(root); });
  const review = signal(props.page === "review");
  const ready = resource(async () => {
    const core = await Core.openIsolated();
    const fixture = await core.sources.startNetworkFixture() as { sourceId: string };
    const plugins = await core.plugins.list() as any;
    await core.settings.update({
      scrapersDisabled: plugins.sources.filter((s: any) => s.sourceId !== fixture.sourceId)
        .map((s: any) => s.sourceId),
      scraperOrder: [fixture.sourceId], scrapeMode: "replace",
    });
    let route: Route = { kind: "library" };
    if (props.page === "favorite" || props.page === "offline") {
      const id = core.collections.favorite({
        sourceId: fixture.sourceId, remoteId: "playlist/夜", sourceLabel: "回声音乐库",
        title: "夜航离线快照", description: "收藏后优先显示本地快照，完整刷新后更新。",
      }, [{ source: fixture.sourceId, id: "old", title: "离线缓存曲目", artists: ["林间回声"] }]);
      route = { kind: "playlist", ref: core.collections.key(id) as EntityRef };
      if (props.page === "offline") {
        await core.sources.invoke(fixture.sourceId, "track.get", ["playlist-offline"], {});
      }
    } else {
      root = `/tmp/haiku-m1-03-fixtures/m405-${Date.now()}-${Math.random().toString(36).slice(2)}`;
      await prepareLibrary(root);
      await core.library.addRoots([root]);
      await core.library.scan();
      const handle = await core.library.query({ sort: "title" });
      const ids = await LibraryService.queryIds(handle);
      await core.library.releaseQuery(handle);
      if (props.page === "progress") {
        await core.library.edit([ids[0]], { title: "Slow" });
        await core.scrape.start(ids, { mode: "replace" });
      } else if (props.page === "review") {
        await core.scrape.start(ids, { mode: "replace" });
      }
    }
    return { core, route };
  });
  return <View class="flex-1">
    <If when={!!ready()}>
      <AppShell initialMode="light" initialRoute={ready()!.route}
        initialOverlay={props.page === "settings" ? "settings" : undefined}
        initialSettingsCategory="存储" />
      <If when={review()}><ScrapeReview core={ready()!.core} open onClose={() => review.set(false)} /></If>
    </If>
    <If when={!!ready.error()}><Text>{String(ready.error())}</Text></If>
  </View>;
}
