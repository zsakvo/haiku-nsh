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
import { HistoryPage } from "../pages/HistoryPage";
import { StatsPage } from "../pages/StatsPage";
import { SourceLibraryPage } from "../pages/SourceLibraryPage";
import { SourceDetailPage } from "../pages/SourceDetailPage";
import { createPlugins } from "../state/plugins";
import { NotificationCenter } from "./Diagnostics";
import { Button } from "../ui/Button";
import { Menu, MenuItem } from "../ui/Surfaces";
import { createCoreState } from "../state/core";
import { createNavigation, destinations, routeKey, type Route } from "./navigation";
import { Settings } from "./Settings";
import { CommandPalette } from "./CommandPalette";

export function AppShell(props: {
  initialMode?: AppearanceMode; initialOverlay?: "settings" | "command";
  initialSettingsCategory?: string; initialPluginLogin?: string; initialPluginResult?: unknown; initialRoute?: Route; injectedLibraryFolders?: string[]; initialNowPlaying?: boolean;
  onLyricChange?: (index: number | null) => void;
  initialNotifications?: boolean; initialLogs?: boolean;
} = {}) {
  const appearance = createAppearance(props.initialMode);
  const nav = createNavigation(props.initialRoute);
  const core = createCoreState();
  const plugins = createPlugins();
  const librarySources = () => plugins.value().sources.filter((s) => s.enabled
    && plugins.value().packages.some((p) => p.id === s.pluginId && p.enabled && p.capabilities.includes("library.browse")));
  const nowPlaying = signal(props.initialNowPlaying ?? false);
  const notifications = signal(props.initialNotifications ?? false);
  const overlay = signal<"settings" | "command" | null>(props.initialOverlay ?? null);
  const showSettings = () => { notifications.set(false); nowPlaying.set(false); overlay.set("settings"); };
  const showCommands = () => { notifications.set(false); nowPlaying.set(false); overlay.set("command"); };
  const showNotifications = () => { overlay.set(null); nowPlaying.set(false); notifications.set(true); };
  const closeOverlay = () => overlay.set(null);
  const goBack = () => { if (!overlay() && !nowPlaying()) nav.back(); };
  const goForward = () => { if (!overlay() && !nowPlaying()) nav.forward(); };
  const shortcuts = { back: "mod+[", forward: "mod+]", commands: "mod+k", settings: "mod+," };
  // Bindings and menu items dispatch the same commands. Keymap owns these keys;
  // the menu's shortcut is their matching native display/fallback.
  keymap.bind(shortcuts.back, goBack, { when: "always" });
  keymap.bind(shortcuts.forward, goForward, { when: "always" });
  keymap.bind(shortcuts.commands, showCommands, { when: "always" });
  keymap.bind(shortcuts.settings, showSettings, { when: "always" });
  effect(() => {
    Core.initialize();
    Window.setTitle("Haiku");
    Window.setTitleBar({ style: "hidden", height: token("toolbar-height") });
  });
  effect(() => {
    DesktopMenu.setApplicationMenu([
      { label: "Haiku", submenu: [
        { role: "about" }, { type: "separator" },
        { label: "设置…", shortcut: shortcuts.settings, onSelect: showSettings },
        { type: "separator" }, { role: "hide" }, { role: "hideOthers" }, { role: "showAll" },
        { type: "separator" }, { role: "quit" },
      ] },
      { label: "编辑", submenu: [ { role: "undo" }, { role: "redo" }, { type: "separator" },
        { role: "cut" }, { role: "copy" }, { role: "paste" }, { role: "selectAll" } ] },
      { label: "视图", submenu: [
        { label: "后退", shortcut: shortcuts.back, disabled: !nav.canBack() || !!overlay(), onSelect: goBack },
        { label: "前进", shortcut: shortcuts.forward, disabled: !nav.canForward() || !!overlay(), onSelect: goForward },
        { type: "separator" }, { label: "命令面板…", shortcut: shortcuts.commands, onSelect: showCommands },
        { label: "通知中心…", onSelect: showNotifications },
        { role: "toggleFullScreen" },
      ] },
      { label: "窗口", submenu: [ { role: "minimize" }, { role: "zoom" }, { type: "separator" },
        { label: "关闭窗口", role: "close", shortcut: "mod+w" } ] },
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
        <IconButton sidebar icon="left" label="后退" hint={`后退 · ${keymap.label(shortcuts.back)}`}
          disabled={!nav.canBack()} onPress={goBack} />
        <IconButton sidebar icon="right" label="前进" hint={`前进 · ${keymap.label(shortcuts.forward)}`}
          disabled={!nav.canForward()} onPress={goForward} />
      </SidebarChrome>
      <SidebarSearch>
        <CommandTrigger shortcut={keymap.label(shortcuts.commands)} onPress={showCommands} />
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
            <SidebarItem icon="radio" label={source().displayName || plugins.value().packages.find((p) => p.id === source().pluginId)?.name || source().sourceId}
              selected={nav.current().kind === "source" && (nav.current() as {source:string}).source === source().sourceId}
              onPress={() => nav.navigate({kind:"source",source:source().sourceId})} />}
          </Each></If>
        </View>)}
        <If when={!!core()}><PlaylistSidebar core={core()!} current={nav.current()} navigate={nav.navigate} /></If>
      </View>
      <View class="shrink-0 flex-row items-center justify-end gap-2 px-4 pb-(--dock-clearance)">
        <IconButton sidebar testID="nav-downloads" icon="download" label="下载"
          selected={nav.current().kind === "downloads"} onPress={() => nav.navigate({ kind: "downloads" })} />
        <IconButton sidebar icon="settings" label="打开设置" hint={`设置 · ${keymap.label(shortcuts.settings)}`} onPress={showSettings} />
      </View>
    </ShellSidebar>
    <WindowPanel>
      <ShellToolbar>
        <Menu trigger={<Button label="更多" size="sm" variant="ghost" onPress={() => {}} />}>
          <MenuItem onSelect={showNotifications}><Text>通知中心…</Text></MenuItem>
        </Menu>
      </ShellToolbar>
      <View class="flex-1 min-h-0">
        {destinations.filter((page) => !["library", "favorites", "playlists", "history", "statistics"].includes(page.id)).map((page) => <PlaceholderPage page={page} active={nav.current().kind === page.id} />)}
        <If when={!!core()}>
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
    <Settings open={overlay() === "settings"} onClose={closeOverlay} appearance={appearance} initialCategory={props.initialSettingsCategory} initialPluginLogin={props.initialPluginLogin} initialPluginResult={props.initialPluginResult} initialLogs={props.initialLogs} onLibrary={() => nav.navigate({kind: "library"})} />
    <If when={!!core()}>
      <NotificationCenter core={core()!} open={notifications()} onClose={() => notifications.set(false)} />
      <NowPlaying open={nowPlaying()} onOpenChange={(event) => nowPlaying.set(event.open)} />
      <PlayerDock onLyricChange={(event) => props.onLyricChange?.(event.index ?? null)} onOpenChange={(event) => {
        nowPlaying.set(event.open);
        if (event.open) closeOverlay();
      }} />
    </If>
    <CommandPalette open={overlay() === "command"} onClose={closeOverlay} onSelect={selectCommand}
      items={[
        ...destinations.map((page) => ({ id: page.id, label: `前往${page.label}`, group: "导航" })),
        { id: "settings", label: "打开设置…", group: "应用", shortcut: shortcuts.settings },
        ...modes.map((mode) => ({ id: `mode:${mode.id}`, label: `外观：${mode.label}`, group: "外观", checked: appearance.mode() === mode.id })),
      ]} />
  </ShellFrame>;
}
