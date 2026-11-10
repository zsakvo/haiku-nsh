import { cleanup, Image, List, resource, signal, Text, token, View } from "nsh";
import { CollectionsService, PluginService, TrackList, type Core } from "haiku/native";
import type { EntityRef, Route } from "../shell/navigation";
import { Button } from "../ui/Button";
import { PlaylistEditor } from "./PlaylistEditor";

export function entityRoute(entity: any): Route {
  const r = entity.reference;
  return { kind: r.kind === "toplist" ? "chart" : r.kind,
    ref: PluginService.reference(r.source, r.kind, r.id) as EntityRef };
}

/** Paged entity grids recycle complete rows; navigation uses host-encoded identities. */
export function SourceGrid(props: { items: any[]; navigate: (route: Route) => void; compact?: boolean }) {
  const rows = () => Array.from({ length: Math.ceil(props.items.length / 4) }, (_, i) => props.items.slice(i * 4, i * 4 + 4));
  return <List each={rows()} keyExtractor={(row) => row.map((e) => JSON.stringify(e.reference)).join(",")}
    class="shrink-0 min-h-0" style={{height:Math.min(rows().length,props.compact?1:3)*156}} estimatedItemSize={156}>
    {(row) => <View class="flex-row gap-4 pb-4">
      <Each of={row()} key={(e) => JSON.stringify(e.reference)}>{(entity) =>
        <View class="flex-1 min-w-0 gap-2 items-start">
          <View class="size-24 rounded-(--radius-panel) bg-muted overflow-hidden items-center justify-center">
            <If when={!!entity().cover}><Image src={entity().cover} class="size-24" /></If>
            <If when={!entity().cover}><Text class="text-muted-foreground">♪</Text></If>
          </View>
          <Button class="px-0" label={entity().title} variant="ghost" size="sm" disabled={!entity().reference}
            onPress={() => props.navigate(entityRoute(entity()))} />
        </View>}
      </Each>
    </View>}
  </List>;
}

/** Remote rows share the existing native menus for likes and adding to user playlists. */
export function SourceTracks(props: { core: Core; tracks: any[]; active: boolean; fit?: boolean }) {
  const error = signal(""), creating = signal<any[] | null>(null);
  let retained: number | undefined;
  let disposed = false;
  const handle = resource(() => props.tracks, async (tracks) => {
    const id = await props.core.sources.trackSource(tracks, retained);
    if (disposed) { await props.core.sources.releaseTracks(id); return id; }
    retained = id;
    return id;
  });
  cleanup(() => { disposed = true; if (retained) void props.core.sources.releaseTracks(retained); });
  async function act(work: () => Promise<unknown>) {
    try { error.set(""); await work(); } catch (e) { error.set(String(e)); }
  }
  return <View class={props.fit ? "shrink-0 min-h-0" : "flex-1 min-h-0"}
    style={props.fit ? {height:Math.min(props.tracks.length,8)*token("track-row-height")+token("compact-control-height")+8} : {}}>
    <If when={!!error() || !!handle.error()}><Text class="text-sm text-destructive">{error() || String(handle.error())}</Text></If>
    <If when={!!handle()}>
      <TrackList source={handle()!} active={props.active && !creating()}
        onPlay={(e) => void act(() => props.core.collections.playSource(e.source, e.id))}
        onAction={(e) => void act(async () => {
          const tracks = await CollectionsService.selectedTracks(handle()!, e.ids) as any[];
          if (e.command === "append") await props.core.player.append(tracks);
          if (e.command === "next") for (const t of [...tracks].reverse()) await props.core.player.playNext(t);
          if (e.command === "new-playlist") creating.set(tracks);
        })} />
    </If>
    <If when={!!creating()}><PlaylistEditor core={props.core} tracks={creating()!} onClose={() => creating.set(null)} /></If>
  </View>;
}
