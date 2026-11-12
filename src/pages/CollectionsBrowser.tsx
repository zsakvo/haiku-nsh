import { cleanup, List, signal, Text, token, View } from "nsh";
import { Button as BaseButton } from "nsh/base";
import { focus } from "../ui/Button";
import type { Core } from "haiku/native";
import type { EntityRef, Route } from "../shell/navigation";
import { mirrorCollections } from "../state/collections";
import { SidebarItem } from "../ui/Navigation";
import { Button } from "../ui/Button";
import { ToggleGroup } from "../ui/LibraryControls";
import { Menu, MenuItem } from "../ui/Surfaces";
import { CollectionArtwork } from "../ui/CollectionArtwork";
import { PlaylistEditor } from "./PlaylistEditor";

export function collectionRoute(core: Core, id: string): Route {
  return { kind: "playlist", ref: core.collections.key(id) as EntityRef };
}
/** Scroll ownership stays within the playlist group, above the sidebar's dock reserve. */
export function PlaylistSidebar(props: { core: Core; current: Route; navigate: (route: Route) => void }) {
  const mirror = mirrorCollections(props.core);
  const creating = signal(false);
  return <View class="flex-1 min-h-0 gap-1">
    <View class="shrink-0 flex-row gap-1 items-center">
      <BaseButton testID="nav-playlists" class={`flex-1 items-start min-h-7 rounded-(--radius) px-2 hover:bg-accent active:bg-muted ${focus}`}
        onPress={() => props.navigate({ kind: "playlists" })}>
        <Text style={{ fontSize: token("sidebar-hint-size") }} class="font-medium text-muted-foreground">歌单…</Text>
      </BaseButton>
      <BaseButton class={`min-h-7 rounded-(--radius) px-2 hover:bg-accent active:bg-muted ${focus}`}
        onPress={() => creating.set(true)}>
        <Text style={{ fontSize: token("sidebar-hint-size") }} class="text-muted-foreground">新建…</Text>
      </BaseButton>
    </View>
    <List each={mirror.collections().filter((p) => p.kind === "user" || p.kind === "plugin")} keyExtractor={(p) => p.id}
      estimatedItemSize={32} class="flex-1 min-h-0">
      {(p) => <SidebarItem icon="music" label={p().title}
        selected={props.current.kind === "playlist" && props.current.ref === props.core.collections.key(p().id)}
        onPress={() => props.navigate(collectionRoute(props.core, p().id))} />}
    </List>
    <If when={creating()}><PlaylistEditor core={props.core} onClose={() => creating.set(false)}
      onSaved={(id) => props.navigate(collectionRoute(props.core, id))} /></If>
  </View>;
}
/** User playlist overview. Virtual rows bound both list and three-column grid modes. */
export function CollectionsBrowser(props: { core: Core; active: boolean; navigate: (route: Route) => void }) {
  const mirror = mirrorCollections(props.core);
  const mode = signal((props.core.settings.get() as { playlistsViewMode: string }).playlistsViewMode);
  cleanup(props.core.settings.onChange((next) => mode.set((next as { playlistsViewMode: string }).playlistsViewMode)));
  const creating = signal(false);
  const error = signal("");
  const playlists = () => mirror.collections().filter((p) => p.kind === "user");
  const rows = () => {
    const items = playlists(); const width = mode() === "grid" ? 3 : 1;
    return Array.from({ length: Math.ceil(items.length / width) }, (_, index) => items.slice(index * width, index * width + width));
  };
  const covers = (id: string) => { mirror.revision(); return props.core.collections.artwork(id) as (string | null)[]; };
  function move(id: string, delta: number) {
    try {
      const ids = playlists().map((p) => p.id); const from = ids.indexOf(id); const to = from + delta;
      if (to < 0 || to >= ids.length) return;
      [ids[from], ids[to]] = [ids[to], ids[from]];
      props.core.collections.reorder(ids);
    } catch (e) { error.set(String(e)); }
  }
  return <View testID="page-playlists" class="flex-1 min-h-0 pb-28" style={{ display: props.active ? "flex" : "none", flexDirection: "column" }}>
    <View class="px-(--content-inset) pb-4 gap-3">
      <View class="flex-row items-center gap-2">
        <Text role="header" class="flex-1 text-xl font-semibold text-foreground">歌单</Text>
        <Button label="新建歌单…" size="sm" onPress={() => creating.set(true)} />
      </View>
      <ToggleGroup value={mode()} onChange={(value) => {
        void props.core.settings.update({ playlistsViewMode: value }).catch((e) => error.set(String(e)));
      }}
        items={[{ id: "list", label: "列表" }, { id: "grid", label: "网格" }]} />
      <If when={!!error()}><Text class="text-sm text-destructive">{error()}</Text></If>
    </View>
    <List each={rows()} keyExtractor={(row) => `${mode()}:${row.map((p) => p.id).join(",")}`}
      estimatedItemSize={mode() === "grid" ? 244 : 72} class="flex-1 min-h-0"
      empty={<Text class="p-5 text-sm text-muted-foreground">创建歌单，把不同来源的音乐放在一起。</Text>}>
      {(row) => <View class="flex-row gap-4 px-(--content-inset) pb-3">
        <Each of={row()} key={(p) => p.id}>{(p) => <View class="flex-1 min-w-0 gap-2">
          <View class={mode() === "grid" ? "gap-2" : "flex-row items-center gap-3"}>
            <CollectionArtwork small={mode() === "list"} covers={covers(p().id)} />
            <View class="flex-1 min-w-0 gap-1">
              <Button label={p().title} variant="ghost" class="items-start px-0" onPress={() => props.navigate(collectionRoute(props.core, p().id))} />
              <Text class="text-xs text-muted-foreground">{p().trackCount} 首歌曲</Text>
            </View>
            <Menu trigger={<Button label="歌单操作…" size="sm" variant="ghost" class="items-start px-0" onPress={() => {}} />}>
              <MenuItem onSelect={() => move(p().id, -1)}><Text>歌单上移</Text></MenuItem>
              <MenuItem onSelect={() => move(p().id, 1)}><Text>歌单下移</Text></MenuItem>
            </Menu>
          </View>
        </View>}</Each>
        <If when={mode() === "grid" && row().length < 3}><View class="flex-1 min-w-0" /></If>
        <If when={mode() === "grid" && row().length < 2}><View class="flex-1 min-w-0" /></If>
      </View>}
    </List>
    <If when={creating()}><PlaylistEditor core={props.core} onClose={() => creating.set(false)}
      onSaved={(id) => props.navigate(collectionRoute(props.core, id))} /></If>
  </View>;
}
