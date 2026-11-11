import { cleanup, effect, List, resource, signal, Text, View } from "nsh";
import type { Core } from "haiku/native";
import { CollectionsService, PluginService, TrackList } from "haiku/native";
import { PlaylistEditor } from "./PlaylistEditor";
import { TrackEditor } from "./TrackEditor";
import { Button } from "../ui/Button";
import { SearchField } from "../ui/SearchField";
import { Dialog, Menu, MenuItem } from "../ui/Surfaces";
import { EmptyState } from "../ui/EmptyState";
import type { Route } from "../shell/navigation";

/** History state stays in Rust; the retained page owns only filters, selection and confirmation. */
export function HistoryPage(props: { core: Core; active: boolean; navigate: (route: Route) => void }) {
  const service = props.core.history;
  const revision = signal(0);
  cleanup(service.onChange(() => revision.update((v) => v + 1)));
  const tab = signal("tracks");
  const search = signal("");
  const filter = signal("");
  const selected = signal<string[]>([]);
  const confirming = signal(false);
  const error = signal("");
  const playlistEditor = signal<unknown[] | null>(null);
  const trackEditor = signal<string[] | null>(null);
  const query = () => ({ search: search(), source: filter() });
  const handle = resource(() => service.query(query()));
  cleanup(() => { if (handle()) void service.releaseQuery(handle()!); });
  effect(() => { if (handle()) void service.setQuery(handle()!, query()); });
  const state = () => { revision(); return service.list() as any; };
  const sources = () => { revision(); return service.sources(); };
  const playlists = () => (state().playlists as any[]).filter((p) =>
    (!filter() || p.sourceId === filter()) && p.title.toLowerCase().includes(search().toLowerCase()));
  async function play(id: string) {
    try { await service.playSource(handle()!, id); } catch (e) { error.set(String(e)); }
  }
  async function action(command: string, keys: string[]) {
    try {
      const tracks = await CollectionsService.selectedTracks(handle()!, keys) as any[];
      if (command === "download") await props.core.downloads.enqueue(tracks);
      if (command === "append") await props.core.player.append(tracks);
      if (command === "next") for (const track of [...tracks].reverse()) await props.core.player.playNext(track);
      if (command === "new-playlist") playlistEditor.set(tracks);
      if (command === "edit") trackEditor.set(tracks.map((t) => t.id));
      if (command === "reveal") for (const track of tracks) await props.core.library.reveal(track.id);
    } catch (e) { error.set(String(e)); }
  }
  return <View testID="page-history" class="flex-1 min-h-0 pb-28"
    style={{ display: props.active ? "flex" : "none", flexDirection: "column" }}>
    <View class="px-(--content-inset) gap-3 pb-4">
      <Text role="header" class="text-xl font-semibold text-foreground">历史</Text>
      <View class="flex-row gap-2">
        <Button label="曲目历史" size="sm" variant={tab() === "tracks" ? "primary" : "secondary"} onPress={() => tab.set("tracks")} />
        <Button label="歌单历史" size="sm" variant={tab() === "playlists" ? "primary" : "secondary"} onPress={() => tab.set("playlists")} />
        <Button label="清空历史…" size="sm" onPress={() => confirming.set(true)} />
        <If when={tab() === "tracks" && selected().length > 0}>
          <Button label="下载" accessibilityLabel="下载所选曲目" size="sm" onPress={() => void action("download", selected())} />
          <Button label="删除所选历史" size="sm" onPress={() => selected().forEach((key) => service.remove("tracks", key))} />
        </If>
      </View>
      <View class="flex-row gap-2">
        <View class="flex-1"><SearchField label="筛选历史" value={search()} onChange={(v) => search.set(v)} placeholder="标题、艺术家或专辑" /></View>
        <Menu trigger={<Button label={`历史来源：${filter() || "全部"}`} size="sm" onPress={() => {}} />}>
          <MenuItem onSelect={() => filter.set("")}><Text>全部来源</Text></MenuItem>
          <Each of={sources()} key={(s) => s}>{(s) =>
            <MenuItem onSelect={() => filter.set(s())}><Text>{s()}</Text></MenuItem>}
          </Each>
        </Menu>
      </View>
      <If when={!!error()}><Text class="text-sm text-destructive">{error()}</Text></If>
    </View>
    <If when={tab() === "tracks" && !!handle() && state().tracks.length > 0}>
      <TrackList source={handle()!} active={props.active && tab() === "tracks" && !confirming() && !playlistEditor() && !trackEditor()}
        onPlay={(e) => void play(e.id)} onSelectionChange={(e) => selected.set(e.ids)}
        onAction={(e) => void action(e.command, e.ids)} />
    </If>
    <If when={!!playlistEditor()}><PlaylistEditor core={props.core} tracks={playlistEditor()!} onClose={() => playlistEditor.set(null)} /></If>
    <If when={!!trackEditor()}><TrackEditor core={props.core} ids={trackEditor()!} onClose={() => trackEditor.set(null)} /></If>
    <If when={tab() === "tracks" && !state().tracks.length}>
      <EmptyState title="留下听过的旋律" description="播放音乐后，你可以在这里找到它。" />
    </If>
    <If when={tab() === "playlists"}>
      <List testID="history-playlist-list" each={playlists()} keyExtractor={(p: any) => JSON.stringify([p.kind, p.sourceId, p.id])}
        class="flex-1 min-h-0" estimatedItemSize={64}>
        {(p: () => any) => <View class="flex-row items-center gap-3 px-(--content-inset) py-3">
          <View class="flex-1 gap-1">
            <Button label={p().title} variant="ghost" onPress={() => {
              if (p().kind === "local") props.navigate({ kind: "playlist", ref: props.core.collections.key(p().id) as any });
              else props.navigate({ kind: "playlist", ref: PluginService.reference(p().sourceId, "playlist", p().id) as any });
            }} />
            <Text class="text-xs text-muted-foreground">{`${p().sourceId} · ${p().trackCount == null ? "曲目数未知" : `${p().trackCount} 首`}`}</Text>
          </View>
          <Button label="删除记录" size="sm" onPress={() => service.remove("playlists", JSON.stringify([p().kind, p().sourceId, p().id]))} />
        </View>}
      </List>
      <If when={!playlists().length}><EmptyState title="还没有浏览歌单" description="打开歌单后，会在这里留下记录。" /></If>
    </If>
    <Dialog open={confirming()} onOpenChange={(e) => confirming.set(e.open)} class="w-96 max-w-full self-center mt-24 p-5 gap-4">
      <Text class="text-lg font-semibold text-foreground">清空历史？</Text>
      <Text class="text-sm text-muted-foreground">仅清空当前分类的记录，音乐文件会保留。</Text>
      <View class="flex-row justify-end gap-2">
        <Button label="取消" onPress={() => confirming.set(false)} />
        <Button label="确认清空历史" variant="destructive" onPress={() => { service.clear(tab()); confirming.set(false); }} />
      </View>
    </Dialog>
  </View>;
}
