import { cleanup, effect, Image, Scroll, resource, signal, Text, View } from "nsh";
import type { Core } from "haiku/native";
import type { Route } from "../shell/navigation";
import { mirrorCollections } from "../state/collections";
import { Button } from "../ui/Button";
import { SourceGrid, SourceTracks } from "./SourceContent";
import { Dialog } from "../ui/Surfaces";
import { AutoDownloadControls } from "../ui/AutoDownloadControls";
import { usePageRefresh } from "../shell/pageRefresh";

/** One retained page per EntityRef. Resources reject stale loads; pagination also checks its epoch. */
export function SourceDetailPage(props: { core: Core; reference: string; active: boolean; navigate: (route: Route) => void }) {
  const tracks = signal<any[]>([]), albums = signal<any[]>([]);
  const next = signal<string | null>(null), albumNext = signal<string | null>(null);
  const info = signal<any>(null), canFavorite = signal(false), expanded = signal(false);
  const error = signal(""), busy = signal(false);
  const unfavorite = signal(false);
  const collections = mirrorCollections(props.core);
  let epoch = 0;
  let closed = false;
  let refreshLease: string | undefined;
  function cancelRefresh() {
    if (refreshLease) props.core.sources.cancelFavoriteRefresh(refreshLease);
    refreshLease = undefined;
  }
  cleanup(() => { closed = true; epoch++; cancelRefresh(); });
  effect(() => { if (!props.active) { epoch++; cancelRefresh(); } });
  function showSnapshot(item: any, reference: any) {
    info.set({ reference, title: item.title, description: item.description,
      cover: item.cover, creator: item.sourceLabel, track_count: item.trackCount });
    tracks.set(props.core.collections.tracks(item.id) as any[]);
    next.set(null); albums.set([]); albumNext.set(null); canFavorite.set(true);
  }
  const first = resource(() => props.active ? props.reference : undefined, async (reference) => {
    const version = ++epoch;
    cancelRefresh(); error.set("");
    const [source, kind, id] = JSON.parse(reference) as string[];
    const favorite = (props.core.collections.list() as any[]).find(
      p => p.kind === "plugin" && p.sourceId === source && p.remoteId === id && kind === "playlist");
    if (favorite) {
      const identity = { source, kind, id };
      showSnapshot(favorite, identity);
      const lease = props.core.sources.beginFavoriteRefresh(reference);
      refreshLease = lease;
      try {
        await props.core.sources.refreshFavorite(lease);
        if (closed || version !== epoch || !props.active) return;
        const updated = (props.core.collections.list() as any[]).find(p => p.id === favorite.id);
        if (updated) showSnapshot(updated, identity);
      } catch (_) {
        if (!closed && version === epoch && props.active) error.set("刷新失败，显示离线内容");
      } finally {
        props.core.sources.cancelFavoriteRefresh(lease);
        if (refreshLease === lease) refreshLease = undefined;
      }
      return;
    }
    const result = await props.core.sources.detail(reference, null) as any;
    if (closed || version !== epoch || reference !== props.reference || !props.active) return;
    info.set(result.info); tracks.set(result.tracks.items); next.set(result.tracks.next);
    albums.set(result.albums?.items ?? []); albumNext.set(result.albums?.next ?? null);
    canFavorite.set(result.canFavorite);
    if (result.info?.reference?.kind === "playlist") {
      const r = result.info.reference;
      props.core.history.recordPlaylist({kind:"remote",sourceId:r.source,id:r.id,title:result.info.title,
        cover:result.info.cover,trackCount:result.info.track_count ?? undefined});
    }
  });
  const favoriteId = () => {
    collections.revision();
    const r = info()?.reference;
    return r ? collections.collections().find((p) => p.id === `favorite:${r.source}:${r.id}`)?.id : undefined;
  };
  usePageRefresh(() => props.active, {
    run: () => first.refetch(),
    disabled: () => first.loading() || busy() || unfavorite(),
  });
  async function act(work: () => Promise<unknown>) {
    busy.set(true); error.set("");
    try { await work(); } catch (e) { error.set(String(e)); } finally { busy.set(false); }
  }
  async function more(kind: "tracks" | "albums") {
    const version = epoch, reference = props.reference;
    const result = kind === "tracks"
      ? await props.core.sources.detail(reference, next()) as any
      : await props.core.sources.artistAlbums(reference, albumNext()) as any;
    if (closed || version !== epoch || reference !== props.reference || !props.active) return;
    if (kind === "tracks") { tracks.set([...tracks(), ...result.tracks.items]); next.set(result.tracks.next); }
    else { albums.set([...albums(), ...result.items]); albumNext.set(result.next); }
  }
  async function play(shuffle: boolean) {
    await props.core.player.setMode(shuffle ? "shuffle" : "sequential");
    await props.core.player.play(tracks(), 0);
  }
  return <View testID="page-source-detail" class="flex-1 min-h-0 pb-28"
    style={{ display: props.active ? "flex" : "none", flexDirection: "column" }}>
    <Scroll class="flex-1" contentClass="flex-col">
    <If when={first.loading() && !info()}>
      <View accessibilityLabel="正在加载详情" class="px-(--content-inset) gap-4">
        <View class="flex-row gap-5"><View class="size-32 rounded-(--radius-panel) bg-muted" />
          <View class="flex-1 gap-3"><View class="h-6 w-64 bg-muted rounded-(--radius)" /><View class="h-4 w-48 bg-muted rounded-(--radius)" /></View>
        </View>
        {[0,1,2,3].map(() => <View class="h-12 bg-muted rounded-(--radius)" />)}
      </View>
    </If>
    <If when={!!first.error() || !!error()}>
      <View class="px-(--content-inset) gap-2">
        <Text class="text-sm text-destructive">{error() || String(first.error())}</Text>
        <Button label="重试详情" size="sm" onPress={() => { error.set(""); first.refetch(); }} />
      </View>
    </If>
    <If when={!!info()}>
      <View class="px-(--content-inset) pb-4 gap-3">
        <View class="flex-row gap-5 items-start">
          <View class="size-32 shrink-0 bg-muted rounded-(--radius-panel) overflow-hidden items-center justify-center">
            <If when={!!info().cover}><Image src={info().cover} class="size-32" /></If>
            <If when={!info().cover}><Text class="text-muted-foreground">♪</Text></If>
          </View>
          <View class="flex-1 min-w-0 gap-2">
            <Text role="header" class="text-xl font-semibold text-foreground">{info().title}</Text>
            <Text class="text-sm text-muted-foreground">{info().creator || info().subtitle || (info().artists ?? []).map((a: any) => typeof a === "string" ? a : a.name).join(" / ")}</Text>
            <If when={info().track_count != null}><Text class="text-xs text-muted-foreground">{info().track_count} 首歌曲</Text></If>
            <View class="flex-row gap-2 flex-wrap">
              <Button label="播放全部" size="sm" variant="primary" disabled={!tracks().length || busy()} onPress={() => void act(() => play(false))} />
              <If when={info().reference?.kind !== "artist"}>
                <Button label="下载全部" size="sm" disabled={busy()}
                  onPress={() => void act(() => props.core.downloads.enqueue(info().reference))} />
              </If>
              <Button label="随机播放" size="sm" disabled={!tracks().length || busy()} onPress={() => void act(() => play(true))} />
              <If when={canFavorite()}><Button label={favoriteId() ? "取消收藏歌单" : "收藏歌单"} size="sm" loading={busy()}
                onPress={() => favoriteId() ? unfavorite.set(true)
                  : void act(() => props.core.sources.favoritePlaylist(props.reference, true))} /></If>
            </View>
          </View>
        </View>
        <If when={!!info().description}>
          <Text style={{ numberOfLines: expanded() ? 0 : 2 }} class="text-sm text-muted-foreground">{info().description}</Text>
          <View class="flex-row"><Button label={expanded() ? "收起描述" : "展开描述"} size="sm" variant="ghost" onPress={() => expanded.set(!expanded())} /></View>
        </If>
        <If when={!!favoriteId()}>
          <AutoDownloadControls core={props.core} id={favoriteId()!} />
        </If>
        <If when={info().reference?.kind === "artist"}>
          <Text class="text-sm font-semibold text-foreground">专辑</Text>
          <SourceGrid items={albums()} navigate={props.navigate} compact />
          <If when={!!albumNext()}><Button label="加载更多专辑" size="sm" loading={busy()} onPress={() => void act(() => more("albums"))} /></If>
          <Text class="text-sm font-semibold text-foreground">热门曲目</Text>
        </If>
      </View>
      <SourceTracks core={props.core} tracks={tracks()} active={props.active} fit />
      <View class="px-(--content-inset) py-2 flex-row gap-3 items-center">
        <Text class="text-xs text-muted-foreground">{`已加载 ${tracks().length} 首`}</Text>
        <If when={!!next()}><Button label="加载更多曲目" size="sm" loading={busy()} onPress={() => void act(() => more("tracks"))} /></If>
      </View>
    </If>
    </Scroll>
    <Dialog open={unfavorite()} onOpenChange={event => { if (!event.open) unfavorite.set(false); }}
      class="w-112 p-5 gap-4">
      <Text role="header" class="text-lg font-semibold text-foreground">取消收藏歌单？</Text>
      <Text class="text-sm text-muted-foreground">仅移除此设备的收藏与离线快照，远端歌单保持不变。</Text>
      <View class="flex-row justify-end gap-2">
        <Button label="取消" onPress={() => unfavorite.set(false)} />
        <Button label="确认取消收藏" variant="destructive" onPress={() => void act(async () => {
          cancelRefresh();
          await props.core.sources.favoritePlaylist(props.reference, false);
          unfavorite.set(false);
        })} />
      </View>
    </Dialog>
  </View>;
}
