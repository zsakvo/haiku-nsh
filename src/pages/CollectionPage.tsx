import { cleanup, effect, resource, signal, Text, View } from "nsh";
import { CollectionsService, Core, TrackList } from "haiku/native";
import { mirrorCollections } from "../state/collections";
import { mirrorCore } from "../state/core";
import { Button } from "../ui/Button";
import { CollectionArtwork } from "../ui/CollectionArtwork";
import { EmptyState } from "../ui/EmptyState";
import { Menu, MenuItem, Dialog } from "../ui/Surfaces";
import { SearchField } from "../ui/SearchField";
import { PlaylistEditor } from "./PlaylistEditor";
import { TrackEditor } from "./TrackEditor";

/** Retained likes/user playlist page with a Rust-owned virtual source and selection. */
export function CollectionPage(props: { core: Core; id: string; active: boolean; onDeleted: () => void }) {
  const service = props.core.collections;
  const mirror = mirrorCollections(props.core);
  const player = mirrorCore(props.core).player;
  const search = signal("");
  const filter = signal("");
  const selected = signal<string[]>([]);
  const editor = signal<unknown[] | "edit" | null>(null);
  const trackEditor = signal<string[] | null>(null);
  const deleting = signal(false);
  const error = signal("");
  const collection = () => mirror.collections().find((p) => p.id === props.id);
  effect(() => {
    const item = collection();
    if (props.active && item && props.id !== "__liked__") {
      props.core.history.recordPlaylist({ kind: "local", sourceId: "local", id: item.id,
        title: item.title, trackCount: item.trackCount });
    }
  });
  const liked = props.id === "__liked__";
  const query = () => ({ id: props.id, search: search(), source: filter() });
  const source = resource(() => service.query(query()));
  cleanup(() => { if (source()) void service.releaseQuery(source()!); });
  effect(() => {
    const handle = source(); const value = query();
    if (handle) void service.setQuery(handle, value).catch((e) => error.set(String(e)));
  });
  const covers = () => { mirror.revision(); return service.artwork(props.id) as (string | null)[]; };
  const sources = () => { mirror.revision(); return service.sources(props.id) as string[]; };
  async function act(action: () => unknown) {
    error.set(""); try { await action(); } catch (e) { error.set(String(e)); }
  }
  async function selectedTracks(keys = selected()) {
    return source() ? await CollectionsService.selectedTracks(source()!, keys) as any[] : [];
  }
  async function command(name: string, keys: string[]) {
    await act(async () => {
      if (name === "new-playlist") { editor.set(await selectedTracks(keys)); return; }
      if (name === "remove") { service.remove(props.id, keys); return; }
      const tracks = await selectedTracks(keys);
      if (name === "append") await props.core.player.append(tracks);
      if (name === "next") for (const track of [...tracks].reverse()) await props.core.player.playNext(track);
      if (name === "edit") { trackEditor.set(tracks.map((t) => t.id)); return; }
      if (name === "reveal") for (const track of tracks) await props.core.library.reveal(track.id);
      if (name === "move-up" || name === "move-down") {
        const all = service.tracks(props.id) as any[];
        const at = all.findIndex((t) => t.source === tracks[0]?.source && t.id === tracks[0]?.id);
        const to = name === "move-up" ? at - 1 : at + 1;
        if (at >= 0 && to >= 0 && to < all.length) service.moveTrack(props.id, at, to);
      }
    });
  }
  return <View testID={liked ? "page-favorites" : "page-playlist"}
    class="flex-1 min-h-0 pb-28" style={{ display: props.active ? "flex" : "none", flexDirection: "column" }}>
    <View class="px-(--content-inset) pb-4 gap-4">
      <View class="flex-row gap-5 items-start">
        <If when={!liked}><CollectionArtwork covers={covers()} /></If>
        <View class="flex-1 min-w-0 gap-2">
          <Text role="header" class="text-xl font-semibold text-foreground">{collection()?.title ?? (liked ? "喜欢" : "歌单已删除")}</Text>
          <If when={!!collection()?.description}><Text class="text-sm text-muted-foreground">{collection()?.description}</Text></If>
          <Text class="text-xs text-muted-foreground">{collection()?.trackCount ?? 0} 首歌曲</Text>
          <View class="flex-row flex-wrap gap-2">
            <Button label="播放全部" variant="primary" size="sm" disabled={!collection()?.trackCount}
              onPress={() => void act(() => service.playSource(source()!, ""))} />
            <If when={collection()?.kind === "user"}>
              <Button label="编辑歌单…" size="sm" onPress={() => editor.set("edit")} />
              <Button label="删除歌单…" size="sm" onPress={() => deleting.set(true)} />
            </If>
          </View>
        </View>
      </View>
      <View class="flex-row gap-2 items-center">
        <View class="flex-1"><SearchField label={liked ? "筛选喜欢" : "筛选歌单"}
          value={search()} placeholder="标题、艺术家或专辑" onChange={(value) => search.set(value)} /></View>
        <Menu trigger={<Button label={`来源：${filter() || "全部"}`} size="sm" onPress={() => {}} />}>
          <MenuItem onSelect={() => filter.set("")}><Text>全部来源</Text></MenuItem>
          <Each of={sources()} key={(source) => source}>{(source) =>
            <MenuItem onSelect={() => filter.set(source())}><Text>{source()}</Text></MenuItem>}
          </Each>
        </Menu>
      </View>
      <If when={!!selected().length}>
        <View class="flex-row flex-wrap gap-2 items-center">
          <Text class="text-xs text-muted-foreground">已选择 {selected().length} 首</Text>
          <Button label={liked ? "取消喜欢所选" : "移除所选"} size="sm"
            onPress={() => void command("remove", selected())} />
          <Menu trigger={<Button label="加入歌单…" size="sm" onPress={() => {}} />}>
            <Each of={mirror.collections().filter((p) => p.kind === "user")} key={(p) => p.id}>{(p) =>
              <MenuItem onSelect={() => void act(async () => service.add(p().id, await selectedTracks()))}>
                <Text>{p().title}</Text>
              </MenuItem>}
            </Each>
            <MenuItem onSelect={() => void command("new-playlist", selected())}><Text>新建歌单…</Text></MenuItem>
          </Menu>
        </View>
      </If>
      <If when={!!error() || !!source.error()}><Text class="text-sm text-destructive">{error() || String(source.error())}</Text>
        <Button label="重试" size="sm" onPress={() => { error.set(""); source.refetch(); }} />
      </If>
    </View>
    <If when={!collection()?.trackCount}>
      <EmptyState title={liked ? "收藏值得重听的音乐" : "从一首歌开始"}
        description={liked ? "点击底栏的喜欢按钮，或在曲目菜单中选择喜欢。" : "在曲目菜单中选择“加入歌单”，把音乐放在这里。"} />
    </If>
    <If when={!!collection()?.trackCount && !!source()}>
      <TrackList source={source()!} active={props.active && !editor() && !deleting() && !trackEditor()}
        currentId={player()?.current ? JSON.stringify([(player().current as any).source, "track", player().current!.id]) : null}
        onPlay={(event) => void act(() => service.playSource(event.source, event.id))}
        onAction={(event) => void command(event.command, event.ids)}
        onSelectionChange={(event) => selected.set(event.ids)} />
    </If>
    <If when={!!editor()}><PlaylistEditor core={props.core}
      collection={editor() === "edit" ? collection() : undefined}
      tracks={Array.isArray(editor()) ? editor() as unknown[] : undefined} onClose={() => editor.set(null)} />
    </If>
    <If when={!!trackEditor()}><TrackEditor core={props.core} ids={trackEditor()!} onClose={() => trackEditor.set(null)} /></If>
    <Dialog open={deleting()} onOpenChange={(event) => { if (!event.open) deleting.set(false); }} class="w-112 p-5 gap-4">
      <Text role="header" class="text-lg font-semibold text-foreground">删除歌单？</Text>
      <Text class="text-sm text-muted-foreground">“{collection()?.title}”将被删除，原始音乐文件会保留。</Text>
      <View class="flex-row justify-end gap-2">
        <Button label="取消" onPress={() => deleting.set(false)} />
        <Button label="确认删除歌单" variant="destructive" onPress={() => void act(() => {
          service.delete(props.id); deleting.set(false); props.onDeleted();
        })} />
      </View>
    </Dialog>
  </View>;
}
