import { cleanup, effect, keymap, signal, Text, token, View } from "nsh";
import { Menu as DesktopMenu, Window } from "nsh/desktop";
import { Core, PluginService, PlayerDock, NowPlaying } from "haiku/native";
import { createAppearance, modes, type AppearanceMode } from "../ui/theme";
import { SidebarItem, SectionLabel } from "../ui/Navigation";
import { IconButton } from "../ui/IconButton";
import { CommandTrigger } from "../ui/SearchField";
import { SidebarChrome, SidebarSearch, ShellFrame, ShellSidebar, ShellToolbar, WindowPanel } from "../ui/Chrome";
import { PlaceholderPage } from "../pages/PlaceholderPage";
import { CollectionPage } from "../pages/CollectionPage";
import { CollectionsBrowser, PlaylistSidebar } from "../pages/CollectionsBrowser";
import { LibraryPage } from "../pages/LibraryPage";
import { DownloadsPage } from "../pages/DownloadsPage";
import { HistoryPage } from "../pages/HistoryPage";
import { StatsPage } from "../pages/StatsPage";
import { SourceLibraryPage } from "../pages/SourceLibraryPage";
import { HomePage, CategoriesPage, RadioPage } from "../pages/DiscoveryPages";
import { SearchPage } from "../pages/SearchPage";
import type { SearchRequest } from "../state/search";
import { SourceDetailPage } from "../pages/SourceDetailPage";
import { createPlugins } from "../state/plugins";
import { NotificationCenter } from "./Diagnostics";
import { AboutContent } from "./About";
import { nshChord, type ShortcutBinding } from "../state/shortcuts";
import { Dialog } from "../ui/Surfaces";
import { Button } from "../ui/Button";
import { Menu, MenuItem } from "../ui/Surfaces";
import { createCoreState } from "../state/core";
import { createNavigation, destinations, routeKey, type Route } from "./navigation";
import { Settings } from "./Settings";
import { CommandPalette } from "./CommandPalette";
import { LyricsSearch } from "./LyricsSearch";

export function AppShell(props: {
  initialMode?: AppearanceMode; initialOverlay?: "settings" | "command";
  initialDownloadDirectory?: string;
  initialSettingsCategory?: string; initialShortcutConflict?: string; initialPluginLogin?: string; initialPluginResult?: unknown; initialRoute?: Route; injectedLibraryFolders?: string[]; initialNowPlaying?: boolean;
  onLyricChange?: (index: number | null) => void;
  initialNotifications?: boolean; initialLogs?: boolean; initialSearch?: SearchRequest; initialCommandQuery?: string;
  backupPath?: string; initialBackupDialog?: "export" | "import";
  initialLyricsSearch?: boolean;
} = {}) {
  const appearance = createAppearance(props.initialMode);
  const nav = createNavigation(props.initialRoute);
  const core = createCoreState();
  const searchRequest=signal<SearchRequest|undefined>(props.initialSearch);
  function openSearch(request:SearchRequest){searchRequest.set({...request});nav.navigate({kind:"search"});}
  const plugins = createPlugins();
  const librarySources = () => plugins.value().sources.filter((s) => s.enabled
    && plugins.value().packages.some((p) => p.id === s.pluginId && p.enabled && p.capabilities.includes("library.browse")));
  const nowPlaying = signal(props.initialNowPlaying ?? false);
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
  const shortcuts = { back: "mod+[", forward: "mod+]", commands: "mod+k", settings: "mod+," };
  const appBindings = signal<Record<string,string>>({});
  const installed=new Map<string,{chord:string;stop:()=>void}>();
  const handlers:Record<string,()=>void>={navigateBack:goBack,navigateForward:goForward,openCommandPalette:showCommands,openSettings:showSettings,closeWindow:()=>Window.close()};
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
  effect(()=>{
    const c=core();if(c) cleanup(c.settings.onOpenLyrics(()=>{
      settingsCategory.set("歌词");settingsRequest.update(v=>v+1);showSettings();
    }));
  });
  const binding=(id:string,fallback:string)=>appBindings()[id]??fallback;
  effect(() => {
    Core.initialize();
    Window.setTitle("Haiku");
    Window.setTitleBar({ style: "hidden", height: token("toolbar-height") });
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
    if (id.startsWith("mode:")) void appearance.setMode(id.slice(5) as AppearanceMode);
    else {
      const page = destinations.find((entry) => entry.id === id);
      if (page) nav.navigate({ kind: page.id });
    }
    closeOverlay();
  }
  return <ShellFrame>
    <ShellSidebar>
      <SidebarChrome>
        <IconButton sidebar icon="left" label="后退" hint={`后退 · ${keymap.label(binding("navigateBack",shortcuts.back))}`}
          disabled={!nav.canBack()} onPress={goBack} />
        <IconButton sidebar icon="right" label="前进" hint={`前进 · ${keymap.label(binding("navigateForward",shortcuts.forward))}`}
          disabled={!nav.canForward()} onPress={goForward} />
      </SidebarChrome>
      <SidebarSearch>
        <CommandTrigger shortcut={keymap.label(binding("openCommandPalette",shortcuts.commands))} onPress={showCommands} />
      </SidebarSearch>
      <View class="flex-1 min-h-0 gap-(--navigation-group-gap) px-2 pb-4">
        {["发现", "音乐库"].map((group) => <View class="gap-(--navigation-row-gap)">
          <SectionLabel label={group} />
          {destinations.filter((page) => page.group === group).map((page) =>
            <SidebarItem testID={`nav-${page.id}`} icon={page.icon} label={page.label}
              selected={nav.current().kind === page.id || (page.id === "library" && "ref" in nav.current()
                && !PluginService.isRemote((nav.current() as {ref:string}).ref) && ["album", "artist"].includes(nav.current().kind))}
              onPress={() => nav.navigate({ kind: page.id })} />)}
          <If when={group === "音乐库"}><Each of={librarySources()} key={(s) => s.sourceId}>{(source) =>
            <SidebarItem icon="folder" label={source().displayName || plugins.value().packages.find((p) => p.id === source().pluginId)?.name || source().sourceId}
              selected={nav.current().kind === "source" && (nav.current() as {source:string}).source === source().sourceId}
              onPress={() => nav.navigate({kind:"source",source:source().sourceId})} />}
          </Each></If>
        </View>)}
        <If when={!!core()}><PlaylistSidebar core={core()!} current={nav.current()} navigate={nav.navigate} /></If>
      </View>
      <View class="shrink-0 flex-row items-center justify-end gap-2 px-4 pb-(--dock-clearance)">
        <IconButton sidebar testID="nav-downloads" icon="download" label="下载"
          selected={nav.current().kind === "downloads"} onPress={() => nav.navigate({ kind: "downloads" })} />
        <IconButton sidebar icon="settings" label="打开设置" hint={`设置 · ${keymap.label(binding("openSettings",shortcuts.settings))}`} onPress={showSettings} />
      </View>
    </ShellSidebar>
    <WindowPanel>
      <ShellToolbar>
        <Menu trigger={<Button label="更多" size="sm" variant="ghost" onPress={() => {}} />}>
          <MenuItem onSelect={showNotifications}><Text>通知中心…</Text></MenuItem>
        </Menu>
      </ShellToolbar>
      <View class="flex-1 min-h-0">
        {destinations.filter((page) => !["home", "categories", "radio", "search", "library", "favorites", "playlists", "history", "statistics", "downloads"].includes(page.id)).map((page) => <PlaceholderPage page={page} active={nav.current().kind === page.id} />)}
        <If when={!!core()}>
          <HomePage core={core()!} active={nav.current().kind==="home"} snapshot={plugins.value()} route={nav.current()} navigate={nav.navigate} settings={showSettings} />
          <CategoriesPage core={core()!} active={nav.current().kind==="categories"} snapshot={plugins.value()} route={nav.current()} navigate={nav.navigate} settings={showSettings} />
          <RadioPage core={core()!} active={nav.current().kind==="radio"} snapshot={plugins.value()} route={nav.current()} navigate={nav.navigate} settings={showSettings} />
          <SearchPage core={core()!} active={nav.current().kind==="search"} snapshot={plugins.value()} request={searchRequest()} navigate={nav.navigate} settings={showSettings} />
          <DownloadsPage core={core()!} active={nav.current().kind === "downloads"} />
          <HistoryPage core={core()!} active={nav.current().kind === "history"} navigate={nav.navigate} />
          <StatsPage core={core()!} active={nav.current().kind === "statistics"} />
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
          <LibraryPage core={core()!} active={nav.current().kind === "library"}
            injectedFolders={props.injectedLibraryFolders} navigate={nav.navigate} />
          <Each of={nav.history().filter((route, index, routes) => "ref" in route
            && ["album", "artist"].includes(route.kind)
            && !PluginService.isRemote(route.ref)
            && routes.findIndex((other) => routeKey(route) === routeKey(other)) === index)}
            key={routeKey}>
            {(route) => <LibraryPage core={core()!}
              active={routeKey(nav.current()) === routeKey(route())}
              group={"ref" in route() ? (route() as Extract<Route, { ref: unknown }>).ref : undefined}
              groupKind={route().kind as "album" | "artist"} navigate={nav.navigate} />}
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
        <If when={nav.current().kind === "library" && !core()}>
          <Text class="px-5 text-sm text-muted-foreground">{core.error() ? String(core.error()) : "正在加载曲库…"}</Text>
        </If>
      </View>
    </WindowPanel>
    <Settings categoryRequest={settingsRequest()} initialDownloadDirectory={props.initialDownloadDirectory} open={overlay() === "settings"} onClose={closeOverlay} appearance={appearance} initialCategory={settingsCategory()} initialShortcutConflict={props.initialShortcutConflict} initialPluginLogin={props.initialPluginLogin} initialPluginResult={props.initialPluginResult} initialLogs={props.initialLogs} backupPath={props.backupPath} initialBackupDialog={props.initialBackupDialog} onLibrary={() => nav.navigate({kind: "library"})} />
    <Dialog open={about()} onOpenChange={(e)=>about.set(e.open)} class="w-160 max-w-full self-center mt-24 p-5 gap-4"><AboutContent /><Button label="关闭关于" onPress={()=>about.set(false)} /></Dialog>
    <If when={!!core()}>
      <NotificationCenter core={core()!} open={notifications()} onClose={() => notifications.set(false)} />
      <NowPlaying open={nowPlaying() && !lyricsSearch()}
        onFindLyrics={() => lyricsSearch.set(true)}
        onOpenChange={(event) => nowPlaying.set(event.open)} />
      <LyricsSearch core={core()!} open={lyricsSearch()} onClose={() => lyricsSearch.set(false)} />
      <PlayerDock onLyricChange={(event) => props.onLyricChange?.(event.index ?? null)} onOpenChange={(event) => {
        nowPlaying.set(event.open);
        if (event.open) closeOverlay();
      }} />
    </If>
    <CommandPalette core={core()??undefined} initialQuery={props.initialCommandQuery} navigate={nav.navigate} search={openSearch} open={overlay() === "command"} onClose={closeOverlay} onSelect={selectCommand}
      items={[
        ...destinations.map((page) => ({ id: page.id, label: `前往${page.label}`, group: "导航" })),
        { id: "settings", label: "打开设置…", group: "应用", shortcut: binding("openSettings",shortcuts.settings) },
        ...modes.map((mode) => ({ id: `mode:${mode.id}`, label: `外观：${mode.label}`, group: "外观", checked: appearance.mode() === mode.id })),
      ]} />
  </ShellFrame>;
}
