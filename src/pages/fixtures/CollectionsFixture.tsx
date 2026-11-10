import { cleanup, resource, signal, Text, View } from "nsh";
import { Buffer } from "buffer";
import { writeFile } from "fs/promises";
import { Core, LibraryService } from "haiku/native";
import { AppShell } from "../../shell/AppShell";
import { Button } from "../../ui/Button";
import { collectionRoute } from "../CollectionsBrowser";
import { prepareLibrary, removeLibrary } from "./libraryAudio";
import { collectionCover } from "./collectionCover";

/** Local silent media and a Core data root unique to this test/preview. */
export function CollectionsFixture(props: { playlist?: boolean; controls?: boolean; initiallyLiked?: boolean }) {
  const ready = resource(async () => {
    const root = `/tmp/haiku-m1-03-fixtures/m3-01-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    await prepareLibrary(root, 1);
    await writeFile(`${root}/cover.png`, Buffer.from(collectionCover, "base64"));
    await writeFile(`${root}/cover-small.png`, Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl6OCEAAAAASUVORK5CYII=", "base64"));
    const core = await Core.openIsolated();
    await core.player.clear();
    await core.player.setVolume(0);
    await core.library.addRoots([root]);
    await core.library.scan();
    const handle = await core.library.query({ search: "", sort: "path" });
    const ids = await LibraryService.queryIds(handle);
    await core.library.releaseQuery(handle);
    const tracks: any[] = [];
    for (let index = 0; index < 4; index++) {
      const id = ids.find((id) => id.endsWith(`/${index}.flac`))!;
      await core.library.edit([id], { cover: `${root}/cover.png` });
      tracks.push(core.library.track(id));
    }
    const id = core.collections.create("周末慢听");
    core.collections.rename(id, "周末慢听", "把熟悉的旋律，留给慢下来的时刻。");
    core.collections.add(id, tracks);
    for (const track of tracks.slice(1)) core.collections.setLiked(track, true);
    if (props.initiallyLiked) core.collections.setLiked(tracks[0], true);
    await core.player.append(tracks);
    await core.player.pause();
    return { root, core, tracks, id };
  });
  return <View class="flex-1">
    <If when={!!ready()}>
      <AppShell initialMode="light" initialRoute={props.playlist
        ? collectionRoute(ready()!.core, ready()!.id) : { kind: "favorites" }} />
      <If when={props.controls}><Probe {...ready()!} /></If>
    </If>
    <If when={!!ready.error()}><Text>{String(ready.error())}</Text></If>
  </View>;
}
function Probe(props: { core: Core; root: string; tracks: any[]; id: string }) {
  const callbacks = signal(0);
  const firstArtwork = signal(0);
  const changedMetadata = signal(false);
  cleanup(props.core.collections.onChange(() => {
    const images = props.core.collections.artwork(props.id);
    firstArtwork.set(images[0]?.length ?? 0);
  }));
  const stopped = signal(false);
  const persisted = signal(false);
  const cleaned = signal(false);
  const stop = props.core.collections.onChange(() => callbacks.update((n) => n + 1));
  cleanup(stop);
  return <View class="flex-row gap-2 flex-wrap">
    <Text>{`first-artwork:${firstArtwork()}`}</Text>
    <Button label="修改本地元数据" onPress={() => void (async () => {
      await props.core.library.edit([props.tracks[0].id], { title: "Song Alpha Edited", cover: `${props.root}/cover-small.png` });
      changedMetadata.set(true);
    })()} />
    <If when={changedMetadata()}><Text>metadata-edited</Text></If>
    <Text testID="collection-events">{`events:${callbacks()};stopped:${stopped()}`}</Text>
    <Button label="停止集合订阅" onPress={() => { stop(); stopped.set(true); }} />
    <Button label="验证重开" onPress={() => void (async () => {
      await props.core.close();
      const reopened = await Core.open();
      if (!(reopened.collections.list() as any[]).some((p) => p.id === props.id)) throw new Error("playlist missing");
      if (!reopened.collections.isLiked(JSON.stringify([props.tracks[0].source, "track", props.tracks[0].id]))) throw new Error("like missing");
      persisted.set(true);
    })()} />
    <Button label="清理集合夹具" onPress={() => void (async () => {
      const core = await Core.open();
      await core.player.clear();
      for (const root of core.library.roots()) await core.library.removeRoot(root);
      await core.library.scan();
      for (const p of core.collections.list() as any[]) if (p.kind !== "liked") core.collections.delete(p.id);
      core.collections.remove("__liked__", core.collections.likedKeys() as string[]);
      await core.collections.flush();
      await removeLibrary(props.root); cleaned.set(true);
    })()} />
    <If when={persisted()}><Text>collections-reopened</Text></If>
    <If when={cleaned()}><Text>collections-cleaned</Text></If>
  </View>;
}
