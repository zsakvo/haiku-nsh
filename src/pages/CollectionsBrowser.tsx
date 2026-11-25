import { cleanup, effect, Image, List, platform, Scroll, signal, Text, token, untracked, View, type Point } from "nsh";
import { Button as BaseButton, Collapsible } from "nsh/base";
import { isAvailable as desktopAvailable, Menu as DesktopMenu } from "nsh/desktop";
import { focus } from "../ui/Button";
import { PluginService, type Core } from "haiku/native";
import type { EntityRef, Route } from "../shell/navigation";
import { mirrorCollections, type Collection } from "../state/collections";
import type { PluginSnapshot } from "../state/plugins";
import { Icon } from "../ui/Icon";
import { IconButton } from "../ui/IconButton";
import { SectionLabel, SidebarItem } from "../ui/Navigation";
import { Button } from "../ui/Button";
import { ToggleGroup } from "../ui/LibraryControls";
import { ContextMenu, Dialog, Menu, MenuItem } from "../ui/Surfaces";
import { CollectionArtwork } from "../ui/CollectionArtwork";
import { PlaylistEditor } from "./PlaylistEditor";

export function collectionRoute(core: Core, id: string): Route {
  return { kind: "playlist", ref: core.collections.key(id) as EntityRef };
}

type SidebarPluginPlaylist = {
  key: string;
  title: string;
  route: Route;
  routeRef: EntityRef;
};

type RemotePlaylistState = {
  signature: string;
  loading: boolean;
  items: { id: string; title: string }[];
  authRequired?: boolean;
  error?: string;
};

/** Scroll ownership stays within the playlist group, above the sidebar's dock reserve. */
export function PlaylistSidebar(props: {
  core: Core;
  snapshot?: PluginSnapshot;
  current: Route;
  navigate: (route: Route) => void;
}) {
  const mirror = mirrorCollections(props.core);
  const creating = signal(false);
  const editing = signal<Collection | null>(null);
  const deleting = signal<Collection | null>(null);
  const menuOpen = signal(false);
  const nativeMenu = platform.os === "macos" && desktopAvailable;
  const collapsed = signal<Record<string, boolean>>({});
  const remoteState = signal<Record<string, RemotePlaylistState>>({});
  let disposed = false;
  cleanup(() => { disposed = true; });

  function downloadPlaylist(playlist: Collection) {
    void props.core.downloads.enqueue(props.core.collections.tracks(playlist.id)).catch(console.error);
  }

  async function openPlaylistMenu(playlist: Collection, at?: Point) {
    if (menuOpen()) return;
    menuOpen.set(true);
    try {
      await DesktopMenu.popup([
        { label: "编辑", onSelect: () => editing.set(playlist) },
        { label: "下载", disabled: !playlist.trackCount, onSelect: () => downloadPlaylist(playlist) },
        { type: "separator" },
        { label: "删除", onSelect: () => deleting.set(playlist) },
      ], at);
    } finally {
      menuOpen.set(false);
    }
  }

  const isExpanded = (id: string) => !collapsed()[id];
  const toggleGroup = (id: string) => collapsed.set({ ...collapsed(), [id]: isExpanded(id) });

  const userPlaylists = () => mirror.collections().filter((p) => p.kind === "user");

  // Sources that declare `me.playlists` capability and are currently enabled.
  const playlistSources = () => {
    const snap = props.snapshot;
    if (!snap) return [];
    return snap.sources.filter((s) => s.enabled).flatMap((s) => {
      const pkg = snap.packages.find((p) => p.id === s.pluginId && p.enabled);
      if (!pkg?.capabilities.includes("me.playlists")) return [];
      return [{
        sourceId: s.sourceId,
        label: s.displayName || pkg.name || s.sourceId,
        logo: pkg.logo,
        needsLogin: !!s.needsLogin,
        signature: `${s.sourceId}:${s.needsLogin ? 1 : 0}:${s.user?.name ?? ""}`,
      }];
    });
  };

  effect(() => {
    const sources = playlistSources();
    const current = untracked(() => remoteState());
    for (const src of sources) {
      if (current[src.sourceId]?.signature === src.signature) continue;
      if (src.needsLogin) {
        remoteState.set({
          ...untracked(() => remoteState()),
          [src.sourceId]: { signature: src.signature, loading: false, items: [], authRequired: true },
        });
        continue;
      }
      remoteState.set({
        ...untracked(() => remoteState()),
        [src.sourceId]: {
          signature: src.signature,
          loading: true,
          items: current[src.sourceId]?.items ?? [],
        },
      });
      const sig = src.signature;
      const sourceId = src.sourceId;
      void props.core.sources.mePlaylists(sourceId).then((raw) => {
        if (disposed) return;
        const latest = untracked(() => remoteState())[sourceId];
        if (latest?.signature !== sig) return;
        const list = Array.isArray(raw) ? raw : [];
        const items = list
          .map((e: any) => ({ id: String(e?.reference?.id ?? e?.id ?? ""), title: String(e?.title ?? "") }))
          .filter((e) => e.id && e.title);
        remoteState.set({
          ...untracked(() => remoteState()),
          [sourceId]: { signature: sig, loading: false, items },
        });
      }).catch((err) => {
        if (disposed) return;
        const latest = untracked(() => remoteState())[sourceId];
        if (latest?.signature !== sig) return;
        const msg = String(err);
        const authRequired = /AUTH_REQUIRED|AUTH_EXPIRED|未登录|登录/i.test(msg);
        remoteState.set({
          ...untracked(() => remoteState()),
          [sourceId]: {
            signature: sig,
            loading: false,
            items: [],
            authRequired,
            error: authRequired ? undefined : msg,
          },
        });
      });
    }
  });

  const pluginGroups = () => {
    const groups = new Map<string, {
      id: string;
      label: string;
      logo?: string;
      loading: boolean;
      authRequired?: boolean;
      error?: string;
      items: SidebarPluginPlaylist[];
      seenRemoteIds: Set<string>;
    }>();

    for (const src of playlistSources()) {
      const state = remoteState()[src.sourceId];
      const seenRemoteIds = new Set<string>();
      const items: SidebarPluginPlaylist[] = [];
      for (const item of state?.items ?? []) {
        seenRemoteIds.add(item.id);
        const ref = PluginService.reference(src.sourceId, "playlist", item.id) as EntityRef;
        items.push({
          key: `remote:${src.sourceId}:${item.id}`,
          title: item.title,
          route: { kind: "playlist", ref },
          routeRef: ref,
        });
      }
      groups.set(src.sourceId, {
        id: src.sourceId,
        label: src.label,
        logo: src.logo,
        loading: state?.loading ?? false,
        authRequired: state?.authRequired,
        error: state?.error,
        items,
        seenRemoteIds,
      });
    }

    // Also include any locally favorited plugin playlists not already returned by `me.playlists`.
    for (const p of mirror.collections()) {
      if (p.kind !== "plugin") continue;
      const key = p.sourceId || p.sourceLabel || "plugin";
      let group = groups.get(key);
      if (!group) {
        const src = props.snapshot?.sources.find((s) => s.sourceId === p.sourceId);
        const pkg = props.snapshot?.packages.find((k) => k.id === src?.pluginId);
        group = {
          id: key,
          label: src?.displayName || pkg?.name || p.sourceLabel || key,
          logo: pkg?.logo,
          loading: false,
          items: [],
          seenRemoteIds: new Set<string>(),
        };
        groups.set(key, group);
      }
      if (p.remoteId && group.seenRemoteIds.has(p.remoteId)) continue;
      const ref = props.core.collections.key(p.id) as EntityRef;
      group.items.push({
        key: `local:${p.id}`,
        title: p.title,
        route: { kind: "playlist", ref },
        routeRef: ref,
      });
    }

    return [...groups.values()];
  };

  return <View class="flex-1 min-h-0">
    <Scroll class="flex-1 min-h-0" contentClass="gap-(--navigation-group-gap)">
      <View class="gap-(--navigation-row-gap)">
        <View class="flex-row items-center justify-between gap-1 px-2 pb-1">
          <Text role="header" style={{ fontSize: token("sidebar-hint-size") }} class="font-medium text-muted-foreground">
            聚合歌单
          </Text>
          <IconButton
            icon="plus"
            label="新建聚合歌单"
            plain
            onPress={() => creating.set(true)}
          />
        </View>
        <SidebarItem
          testID="nav-favorites"
          icon="heart"
          label="喜欢"
          selected={props.current.kind === "favorites"}
          onPress={() => props.navigate({ kind: "favorites" })}
        />
        <Each of={userPlaylists()} key={(p) => p.id}>
          {(p) => {
            const item = <SidebarItem icon="music" label={p().title}
              selected={props.current.kind === "playlist" && props.current.ref === props.core.collections.key(p().id)}
              onPress={() => props.navigate(collectionRoute(props.core, p().id))} />;
            if (nativeMenu) {
              return <View onContextMenu={(e) => void openPlaylistMenu(p(), e.pagePosition)}>{item}</View>;
            }
            return <ContextMenu trigger={item}>
              <MenuItem onSelect={() => editing.set(p())}><Text>编辑</Text></MenuItem>
              <MenuItem disabled={!p().trackCount} onSelect={() => downloadPlaylist(p())}><Text>下载</Text></MenuItem>
              <MenuItem onSelect={() => deleting.set(p())}><Text>删除</Text></MenuItem>
            </ContextMenu>;
          }}
        </Each>
      </View>

      <View class="gap-(--navigation-row-gap)">
        <SectionLabel label="插件歌单" />
        <Each of={pluginGroups()} key={(g) => g.id}>
          {(g) => <Collapsible
            open={isExpanded(g().id)}
            reveal
            content={<View class="pt-(--navigation-row-gap) gap-(--navigation-row-gap)">
              <If when={g().loading && g().items.length === 0}>
                <Text style={{ fontSize: token("sidebar-hint-size") }} class="px-2 py-1 text-muted-foreground">
                  正在加载…
                </Text>
              </If>
              <If when={!g().loading && g().items.length === 0 && !!g().authRequired}>
                <Text style={{ fontSize: token("sidebar-hint-size") }} class="px-2 py-1 text-muted-foreground">
                  登录后查看歌单
                </Text>
              </If>
              <If when={!g().loading && g().items.length === 0 && !g().authRequired && !!g().error}>
                <Text style={{ fontSize: token("sidebar-hint-size") }} class="px-2 py-1 text-muted-foreground">
                  加载失败
                </Text>
              </If>
              <Each of={g().items} key={(p) => p.key}>
                {(p) => <SidebarItem icon="playlist" label={p().title}
                  selected={props.current.kind === "playlist" && props.current.ref === p().routeRef}
                  onPress={() => props.navigate(p().route)} />}
              </Each>
            </View>}
          >
            <BaseButton
              accessibilityLabel={g().label}
              onPress={() => toggleGroup(g().id)}
              class={`flex-row items-center gap-2 rounded-(--radius) min-h-(--navigation-row-height) px-2 py-0.5 active:bg-muted ${focus}`}
            >
              <If when={!!g().logo} else={<Icon name="plug" color="sidebar-foreground" sidebar />}>
                <Image src={g().logo!} class="size-(--sidebar-icon-size) shrink-0 rounded-sm" />
              </If>
              <Text
                style={{ fontSize: token("sidebar-text-size"), numberOfLines: 1, textOverflow: "ellipsis" }}
                class="flex-1 min-w-0 text-sidebar-foreground"
              >
                {g().label}
              </Text>
              <View
                class="shrink-0 transition-transform duration-200 ease-out motion-reduce:transition-none"
                style={{ rotate: isExpanded(g().id) ? "0deg" : "-90deg" }}
              >
                <Icon name="down" class="size-3 shrink-0" color="muted-foreground" />
              </View>
            </BaseButton>
          </Collapsible>}
        </Each>
      </View>
    </Scroll>
    <If when={creating()}><PlaylistEditor core={props.core} onClose={() => creating.set(false)}
      onSaved={(id) => props.navigate(collectionRoute(props.core, id))} /></If>
    <If when={!!editing()}><PlaylistEditor core={props.core} collection={editing()!}
      onClose={() => editing.set(null)} /></If>
    <Dialog open={!!deleting()} onOpenChange={(event) => { if (!event.open) deleting.set(null); }} class="w-112 max-w-full m-auto p-5 gap-4">
      <Text role="header" class="text-lg font-semibold text-foreground">删除歌单？</Text>
      <Text class="text-sm text-muted-foreground">
        {`“${deleting()?.title ?? ""}”将被删除，原始音乐文件会保留。`}
      </Text>
      <View class="flex-row justify-end gap-2">
        <Button label="取消" onPress={() => deleting.set(null)} />
        <Button label="确认删除歌单" variant="destructive" onPress={() => {
          const target = deleting();
          if (!target) return;
          const deletedRef = props.core.collections.key(target.id);
          props.core.collections.delete(target.id);
          deleting.set(null);
          if (props.current.kind === "playlist" && props.current.ref === deletedRef) {
            props.navigate({ kind: "favorites" });
          }
        }} />
      </View>
    </Dialog>
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
  const covers = (id: string) => { mirror.revision(); return props.core.collections.artwork(id, mode() === "list") as (string | null)[]; };
  function move(id: string, delta: number) {
    try {
      const ids = playlists().map((p) => p.id); const from = ids.indexOf(id); const to = from + delta;
      if (to < 0 || to >= ids.length) return;
      [ids[from], ids[to]] = [ids[to], ids[from]];
      props.core.collections.reorder(ids);
    } catch (e) { error.set(String(e)); }
  }
  return <View testID="page-playlists" class="flex-1 min-h-0" style={{ display: props.active ? "flex" : "none", flexDirection: "column" }}>
    <View class="pl-(--content-inset-left) pr-(--content-inset-right) pb-4 gap-3">
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
    <List contentStyle={{paddingBottom:token("dock-scroll-clearance")}} each={rows()} keyExtractor={(row) => `${mode()}:${row.map((p) => p.id).join(",")}`}
      estimatedItemSize={mode() === "grid" ? 244 : 72} class="flex-1 min-h-0"
      empty={<Text class="p-5 text-sm text-muted-foreground">创建歌单，把不同来源的音乐放在一起。</Text>}>
      {(row) => <View class="flex-row gap-4 pl-(--content-inset-left) pr-(--content-inset-right) pb-3">
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
