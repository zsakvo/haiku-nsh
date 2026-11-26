import { cleanup, effect, keymap, platform, signal, Text, token, View } from "nsh";
import { currentWindow, Menu as DesktopMenu } from "nsh/desktop";
import { Scrollbar } from "nsh/base";
import { Core, PluginService, PlayerDock, NowPlaying } from "haiku/native";
import { createAppearance, modes, type AppearanceMode } from "../ui/theme";
import { SidebarItem, SectionLabel } from "../ui/Navigation";
import { IconButton } from "../ui/IconButton";
import { CommandTrigger } from "../ui/SearchField";
import { SidebarChrome, ShellFrame, ShellSidebar, NavigationBar, NavigationLayoutContext, WindowPanel } from "../ui/Chrome";
import { PlaceholderPage } from "../pages/PlaceholderPage";
import { CollectionPage } from "../pages/CollectionPage";
import { CollectionsBrowser, PlaylistSidebar } from "../pages/CollectionsBrowser";
import { DownloadsPage } from "../pages/DownloadsPage";
import { SourceLibraryPage } from "../pages/SourceLibraryPage";
import { HomePage, CategoriesPage } from "../pages/DiscoveryPages";
import { SearchPage } from "../pages/SearchPage";
import type { SearchRequest } from "../state/search";
import { SourceDetailPage } from "../pages/SourceDetailPage";
import { createPlugins } from "../state/plugins";
import { NotificationCenter } from "./Diagnostics";
import { AboutContent } from "./About";
import { nshChord, type ShortcutBinding } from "../state/shortcuts";
import { Dialog } from "../ui/Surfaces";
import { Button } from "../ui/Button";
import { createCoreState, type PlayerSnapshot } from "../state/core";
import { createNavigation, destinations, routeKey, type Route } from "./navigation";
import { Settings } from "./Settings";
import { closeDevTools, openDevTools } from "./DevTools";
import { startTraceCapture, stopTraceCapture } from "../state/devtools";
import { createSettings } from "../state/settings";
import { CommandPalette, type PaletteCommand } from "./CommandPalette";
import { LyricsSearch } from "./LyricsSearch";
import { createPageRefresh, PageRefreshContext } from "./pageRefresh";
import { createPageSource, PageSourceContext } from "./pageSource";
import { TrackEditor } from "../pages/TrackEditor";
import { SourceChip } from "../ui/SourceChip";

export function AppShell(props: {
  initialMode?: AppearanceMode; initialOverlay?: "settings" | "command";
  initialDownloadDirectory?: string;
  initialSettingsCategory?: string; initialShortcutConflict?: string; initialPluginLogin?: string; initialRoute?: Route; initialNowPlaying?: boolean;
  onLyricChange?: (index: number | null) => void;
  initialNotifications?: boolean; initialLogs?: boolean; initialSearch?: SearchRequest; initialCommandQuery?: string;
  backupPath?: string; initialBackupDialog?: "export" | "import";
  initialLyricsSearch?: boolean;
} = {}) {
  const win = currentWindow();
  Scrollbar.setTheme({horizontal:{
    class:"hidden pointer-events-none",thumbClass:"hidden pointer-events-none",
  }});
  const appearance = createAppearance(props.initialMode);
  const nav = createNavigation(props.initialRoute);
  const refresh = createPageRefresh();
  const pageSource = createPageSource();
  const core = createCoreState();
  const player = signal<PlayerSnapshot | null>(null);
  const searchRequest=signal<SearchRequest|undefined>(props.initialSearch);
  function openSearch(request:SearchRequest){searchRequest.set({...request});nav.navigate({kind:"search"});}
  const plugins = createPlugins();
  const appSettings = createSettings();
  const developerMode = () => !!appSettings.value()?.developerMode;
  effect(() => {
    const c = core();
    if (!appSettings.value()) return;
    if (!developerMode()) { stopTraceCapture(); closeDevTools(); }
    else if (c) startTraceCapture(c);
  });
  const librarySources = () => plugins.value().sources.filter((s) => s.enabled
    && plugins.value().packages.some((p) => p.id === s.pluginId && p.enabled && p.capabilities.includes("library.browse")));
  const nowPlaying = signal(props.initialNowPlaying ?? false);
  const editingTrack = signal<string[] | null>(null);
  const lyricsSearch = signal(props.initialLyricsSearch ?? false);
  const notifications = signal(props.initialNotifications ?? false);
  const about = signal(false);
  const settingsRequest=signal(0);
  const settingsCategory=signal(props.initialSettingsCategory ?? "外观");
  const overlay = signal<"settings" | "command" | null>(props.initialOverlay ?? null);
  const showSettings = () => { notifications.set(false); nowPlaying.set(false); overlay.set("settings"); };
  const showCommands = () => { notifications.set(false); nowPlaying.set(false); overlay.set("command"); };
  const showNotifications = () => { overlay.set(null); nowPlaying.set(false); notifications.set(true); };
  const closeOverlay = () => overlay.set(null);
  const goBack = () => { if (!overlay() && !nowPlaying()) nav.back(); };
  const goForward = () => { if (!overlay() && !nowPlaying()) nav.forward(); };
  const navigationBlocked = () => !!overlay() || nowPlaying() || notifications() || about() || lyricsSearch();
  const canRefresh = () => !navigationBlocked() && !!refresh.current() && !refresh.current()!.disabled();
  const refreshPage = () => { if (canRefresh()) refresh.current()!.run(); };
  const shortcuts = { back: "mod+[", forward: "mod+]", refresh: "mod+shift+r", commands: "mod+k", settings: "mod+," };
  keymap.bind(shortcuts.refresh, refreshPage, { when: "always" });
  const appBindings = signal<Record<string,string>>({});
  const installed=new Map<string,{chord:string;stop:()=>void}>();
  const handlers:Record<string,()=>void>={navigateBack:goBack,navigateForward:goForward,openCommandPalette:showCommands,openSettings:showSettings,closeWindow:()=>win.close()};
  function reconcileBindings(bindings:ShortcutBinding[]) {
    const app=bindings.filter(b=>b.scope==="app");
    for(const b of app) {
      const chord=nshChord(b.chord),previous=installed.get(b.id);
      // Initial onChange delivery and settings refresh must retain unchanged IDs:
      // a platform KeyCommand may already be queued against one of those IDs.
      if(previous && keymap.label(previous.chord)===keymap.label(chord)) continue;
      previous?.stop();
      const stop=keymap.bind(chord,handlers[b.id]??(()=>void core()?.shortcuts.trigger(b.id).catch(console.error)),
        {when:["closeWindow","openCommandPalette","navigateBack","navigateForward","openSettings"].includes(b.id)?"always":"notEditing"});
      installed.set(b.id,{chord,stop});
    }
    appBindings.set(Object.fromEntries(app.map(b=>[b.id,nshChord(b.chord)])));
  }
  reconcileBindings([
    {id:"navigateBack",scope:"app",chord:shortcuts.back},
    {id:"navigateForward",scope:"app",chord:shortcuts.forward},
    {id:"openCommandPalette",scope:"app",chord:shortcuts.commands},
    {id:"openSettings",scope:"app",chord:shortcuts.settings},
    {id:"closeWindow",scope:"app",chord:"mod+w"},
  ]);
  cleanup(()=>{for(const binding of installed.values())binding.stop();});
  effect(() => {
    const c=core(); if(c) cleanup(c.shortcuts.onChange(s=>reconcileBindings((s as {bindings:ShortcutBinding[]}).bindings)));
  });
  effect(() => {
    const c = core();
    if (!c) return;
    player.set(c.player.snapshot() as PlayerSnapshot);
    cleanup(c.player.onState((state) => player.set(state as PlayerSnapshot)));
  });
  effect(()=>{
    const c=core();if(c) cleanup(c.settings.onOpenLyrics(()=>{
      settingsCategory.set("歌词");settingsRequest.update(v=>v+1);showSettings();
    }));
  });
  const binding=(id:string,fallback:string)=>appBindings()[id]??fallback;
  effect(() => {
    Core.initialize();
    win.set({ title: "Haiku", titleBar: { style: "hidden", height: token("toolbar-height") } });
  });
  effect(() => {
    DesktopMenu.setApplicationMenu([
      { label: "Haiku", submenu: [
        { label: "关于 Haiku…", onSelect: () => { overlay.set(null); about.set(true); } }, { type: "separator" },
        { label: "设置…", shortcut: binding("openSettings",shortcuts.settings), onSelect: showSettings },
        { type: "separator" }, { role: "hide" }, { role: "hideOthers" }, { role: "showAll" },
        { type: "separator" }, { role: "quit" },
      ] },
      { label: "编辑", submenu: [ { role: "undo" }, { role: "redo" }, { type: "separator" },
        { role: "cut" }, { role: "copy" }, { role: "paste" }, { role: "selectAll" } ] },
      { label: "视图", submenu: [
        { label: "后退", shortcut: binding("navigateBack",shortcuts.back), disabled: !nav.canBack() || !!overlay(), onSelect: goBack },
        { label: "前进", shortcut: binding("navigateForward",shortcuts.forward), disabled: !nav.canForward() || !!overlay(), onSelect: goForward },
        { label: "刷新", shortcut: shortcuts.refresh, disabled: !canRefresh(), onSelect: refreshPage },
        { type: "separator" }, { label: "命令面板…", shortcut: binding("openCommandPalette",shortcuts.commands), onSelect: showCommands },
        { label: "通知中心…", onSelect: showNotifications },
        { role: "toggleFullScreen" },
      ] },
      { label: "窗口", submenu: [ { role: "minimize" }, { role: "zoom" }, { type: "separator" },
        { label: "关闭窗口", role: "close", shortcut: binding("closeWindow","mod+w") } ] },
    ]);
  });
  cleanup(() => DesktopMenu.setApplicationMenu(null));
  function selectCommand(id: string) {
    if (id === "settings") { showSettings(); return; }
    if (id === "refresh") { closeOverlay(); if (refresh.current() && !refresh.current()!.disabled()) refresh.current()!.run(); return; }
    if (id === "player:toggle") { void core()?.player.toggle().catch(console.error); closeOverlay(); return; }
    if (id === "player:next") { void core()?.player.next().catch(console.error); closeOverlay(); return; }
    if (id === "player:previous") { void core()?.player.previous().catch(console.error); closeOverlay(); return; }
    if (id.startsWith("source:")) nav.navigate({ kind: "source", source: id.slice(7) });
    else if (id.startsWith("mode:")) void appearance.setMode(id.slice(5) as AppearanceMode);
    else {
      const page = destinations.find((entry) => entry.id === id);
      if (page) nav.navigate({ kind: page.id });
    }
    closeOverlay();
  }
  const destinationKeywords: Record<string, string[]> = {
    home: ["首页", "发现", "home", "discover"],
    categories: ["流派", "歌单分类", "categories", "genres"],
    favorites: ["收藏", "红心", "favorites", "liked"],
  };
  const modeKeywords: Record<AppearanceMode, string[]> = {
    system: ["外观", "主题", "自动", "system", "auto", "theme"],
    light: ["外观", "主题", "亮色", "日间", "light", "theme"],
    dark: ["外观", "主题", "暗色", "夜间", "dark", "theme"],
  };
  const commandItems = (): PaletteCommand[] => {
    const current = nav.current();
    const playback = player();
    const playing = playback?.status === "playing" || playback?.status === "loading";
    const list: PaletteCommand[] = [
      // 歌单、下载、搜索不作为命令面板入口（搜索由面板本身承担）。
      ...destinations.filter((page) => !["playlists", "downloads", "search"].includes(page.id)).map((page): PaletteCommand => ({
        id: page.id,
        label: page.label,
        group: "页面",
        icon: page.icon,
        meta: current.kind === page.id ? "当前" : undefined,
        actionLabel: "打开",
        keywords: [`前往${page.label}`, page.group, ...(destinationKeywords[page.id] ?? [])],
      })),
      ...librarySources().map((source): PaletteCommand => {
        const label = source.displayName || plugins.value().packages.find((p) => p.id === source.pluginId)?.name || source.sourceId;
        const active = current.kind === "source" && (current as { source: string }).source === source.sourceId;
        return {
          id: `source:${source.sourceId}`,
          label,
          group: "页面",
          icon: "folder",
          meta: active ? "当前" : "来源",
          actionLabel: "打开",
          keywords: ["来源", "曲库", label],
        };
      }),
    ];
    if (playback?.current) {
      list.push(
        {
          id: "player:toggle",
          label: playing ? "暂停播放" : "继续播放",
          group: "播放",
          icon: playing ? "pause" : "play",
          meta: playback.current.title,
          shortcut: appBindings()["togglePlayback"],
          actionLabel: playing ? "暂停" : "播放",
          keywords: ["播放", "暂停", "play", "pause", playback.current.title],
        },
        {
          id: "player:next",
          label: "下一首",
          group: "播放",
          icon: "skipNext",
          shortcut: appBindings()["nextTrack"],
          actionLabel: "切换",
          keywords: ["切歌", "下一曲", "next"],
        },
        {
          id: "player:previous",
          label: "上一首",
          group: "播放",
          icon: "skipPrev",
          shortcut: appBindings()["previousTrack"],
          actionLabel: "切换",
          keywords: ["切歌", "上一曲", "prev", "previous"],
        },
      );
    }
    if (refresh.current() && !refresh.current()!.disabled()) {
      list.push({
        id: "refresh",
        label: "刷新当前页面",
        group: "操作",
        icon: "sync",
        shortcut: shortcuts.refresh,
        actionLabel: "刷新",
        keywords: ["刷新", "重载", "refresh", "reload"],
      });
    }
    list.push(
      {
        id: "settings",
        label: "设置…",
        group: "操作",
        icon: "settings",
        shortcut: binding("openSettings", shortcuts.settings),
        actionLabel: "打开",
        keywords: ["打开设置", "偏好", "settings", "preferences"],
      },
      ...modes.map((mode): PaletteCommand => ({
        id: `mode:${mode.id}`,
        label: mode.label,
        group: "外观",
        icon: "palette",
        checked: appearance.mode() === mode.id,
        actionLabel: "切换",
        keywords: [`外观：${mode.label}`, ...modeKeywords[mode.id]],
      })),
    );
    return list;
  };
  // macOS: the bar floats over the page, whose content scrolls beneath it.
  const floatingNavigation = platform.os === "macos";
  const scrolledPages = new Set<object>(), contentScrolled = signal(false);
  const navigationLayout = {
    inset: () => floatingNavigation ? token("navigation-bar-height") + token("navigation-content-gap") : 0,
    report: (page: object, scrolled: boolean) => {
      if (scrolled) scrolledPages.add(page); else scrolledPages.delete(page);
      contentScrolled.set(scrolledPages.size > 0);
    },
  };
  const hasSources = () => !!pageSource.current()?.items().length;
  const navigationBar = () => <NavigationBar floating={floatingNavigation} scrolled={contentScrolled()}>
    <IconButton navigation testID="nav-back" icon="left" label="后退" hint={`后退 · ${keymap.label(binding("navigateBack",shortcuts.back))}`}
      disabled={!nav.canBack() || navigationBlocked()} onPress={goBack} />
    <IconButton navigation testID="nav-refresh" icon="sync" label="刷新" hint={`刷新 · ${keymap.label(shortcuts.refresh)}`}
      disabled={!canRefresh()} onPress={refreshPage} />
    <View class="shrink min-w-0 flex-row" style={{ width: token("navigation-search-width") }}>
      <CommandTrigger class="flex-1" shortcut={keymap.label(binding("openCommandPalette",shortcuts.commands))} onPress={showCommands} />
    </View>
    <View class="flex-1" />
    <If when={hasSources()}>
      <SourceChip items={pageSource.current()!.items()} value={pageSource.current()!.value()}
        disabled={navigationBlocked()} onChange={(value)=>pageSource.current()?.select(value)} />
    </If>
  </NavigationBar>;
  return <PageSourceContext.Provider value={pageSource}><PageRefreshContext.Provider value={refresh}>
    <NavigationLayoutContext.Provider value={navigationLayout}><ShellFrame>
    <ShellSidebar>
      <SidebarChrome />
      <View class="flex-1 min-h-0 gap-(--navigation-group-gap) px-2 pb-4">
        <View class="gap-(--navigation-row-gap)">
          <SectionLabel label="发现" />
          {destinations.filter((page) => page.group === "发现" && page.id !== "search").map((page) =>
            <SidebarItem testID={`nav-${page.id}`} icon={page.icon} label={page.label}
              selected={nav.current().kind === page.id}
              onPress={() => nav.navigate({ kind: page.id })} />)}
        </View>
        <If when={!!core()}><PlaylistSidebar core={core()!} snapshot={plugins.value()} current={nav.current()} navigate={nav.navigate} /></If>
      </View>
      {/* Equal left and bottom insets; developer tools sit beside settings in the same row. */}
      <View class="shrink-0 flex-row items-center gap-1 px-2 pb-2">
        <SidebarItem testID="nav-settings" icon="settings" label="设置" fit
          selected={false} onPress={showSettings} />
        <If when={developerMode()}>
          <SidebarItem testID="nav-devtools" icon="code" label="开发工具" fit
            selected={false} onPress={openDevTools} />
        </If>
      </View>
    </ShellSidebar>
    <WindowPanel>
      <If when={!floatingNavigation}>{navigationBar()}</If>
      <View class="flex-1 min-h-0" style={{ paddingTop: navigationLayout.inset() }}>
        {destinations.filter((page) => !["home", "categories", "search", "favorites", "playlists", "downloads"].includes(page.id)).map((page) => <PlaceholderPage page={page} active={nav.current().kind === page.id} />)}
        <If when={!!core()}>
          <HomePage core={core()!} active={nav.current().kind==="home"} snapshot={plugins.value()} route={nav.current()} navigate={nav.navigate} settings={showSettings} />
          <CategoriesPage core={core()!} active={nav.current().kind==="categories"} snapshot={plugins.value()} route={nav.current()} navigate={nav.navigate} settings={showSettings} />
          <SearchPage core={core()!} active={nav.current().kind==="search"} snapshot={plugins.value()} request={searchRequest()} navigate={nav.navigate} settings={showSettings} />
          <DownloadsPage core={core()!} active={nav.current().kind === "downloads"} />
          <CollectionPage core={core()!} id="__liked__" active={nav.current().kind === "favorites"} onDeleted={() => nav.navigate({ kind: "favorites" })} />
          <CollectionsBrowser core={core()!} active={nav.current().kind === "playlists"} navigate={nav.navigate} />
          <Each of={nav.history().filter((route, index, routes) => route.kind === "playlist"
            && !PluginService.isRemote(route.ref)
            && routes.findIndex((other) => routeKey(route) === routeKey(other)) === index)} key={routeKey}>
            {(route) => <CollectionPage core={core()!}
              id={core()!.collections.idForKey((route() as Extract<Route, {ref: unknown}>).ref)}
              active={routeKey(nav.current()) === routeKey(route())}
              onDeleted={() => nav.navigate({ kind: "playlists" })} />}
          </Each>
          <Each of={nav.history().filter((route, index, routes) => route.kind === "source"
            && routes.findIndex((other) => routeKey(route) === routeKey(other)) === index)} key={routeKey}>
            {(route) => <SourceLibraryPage core={core()!} source={(route() as {source:string}).source}
              title={librarySources().find((s) => s.sourceId === (route() as {source:string}).source)?.displayName
                || plugins.value().packages.find((p) => p.id === librarySources().find((s) => s.sourceId === (route() as {source:string}).source)?.pluginId)?.name}
              active={routeKey(nav.current()) === routeKey(route())} navigate={nav.navigate} settings={showSettings} />}
          </Each>
          <Each of={nav.history().filter((route, index, routes) => "ref" in route && PluginService.isRemote(route.ref)
            && routes.findIndex((other) => routeKey(route) === routeKey(other)) === index)} key={routeKey}>
            {(route) => <SourceDetailPage core={core()!} reference={(route() as {ref:string}).ref}
              active={routeKey(nav.current()) === routeKey(route())} navigate={nav.navigate} />}
          </Each>
        </If>
      </View>
      <If when={floatingNavigation}>{navigationBar()}</If>
      <If when={!!core()}>
        <PlayerDock onLyricChange={(event) => props.onLyricChange?.(event.index ?? null)} onOpenChange={(event) => {
          nowPlaying.set(event.open);
          if (event.open) closeOverlay();
        }} />
      </If>
    </WindowPanel>
    <Settings categoryRequest={settingsRequest()} initialDownloadDirectory={props.initialDownloadDirectory} open={overlay() === "settings"} onClose={closeOverlay} appearance={appearance} initialCategory={settingsCategory()} initialShortcutConflict={props.initialShortcutConflict} initialPluginLogin={props.initialPluginLogin} initialLogs={props.initialLogs} backupPath={props.backupPath} initialBackupDialog={props.initialBackupDialog} />
    <Dialog open={about()} onOpenChange={(e)=>about.set(e.open)} class="w-160 p-5 gap-4"><AboutContent /><Button label="关闭关于" onPress={()=>about.set(false)} /></Dialog>
    <If when={!!core()}>
      <NotificationCenter core={core()!} open={notifications()} onClose={() => notifications.set(false)} />
      <If when={!!editingTrack()}><TrackEditor core={core()!} ids={editingTrack()!} onClose={() => editingTrack.set(null)} /></If>
      <NowPlaying open={nowPlaying() && !lyricsSearch() && !editingTrack()}
        onAction={e => {
          const track = e.value as { id?: string; source?: string };
          if (e.action === "edit" && track?.source === "local" && track.id) editingTrack.set([track.id]);
        }}
        onFindLyrics={() => lyricsSearch.set(true)}
        onOpenChange={(event) => nowPlaying.set(event.open)} />
      <LyricsSearch core={core()!} open={lyricsSearch()} onClose={() => lyricsSearch.set(false)} />
    </If>
    <CommandPalette core={core()??undefined} initialQuery={props.initialCommandQuery} navigate={nav.navigate} search={openSearch} open={overlay() === "command"} onClose={closeOverlay} onSelect={selectCommand}
      items={commandItems()} />
  </ShellFrame></NavigationLayoutContext.Provider></PageRefreshContext.Provider></PageSourceContext.Provider>;
}
