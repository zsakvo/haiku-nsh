import { cleanup, effect, keymap, signal, Text, token, View } from "nsh";
import { Menu as DesktopMenu, Window } from "nsh/desktop";
import { Core, PlayerDock, NowPlaying } from "haiku/native";
import { createAppearance, modes, type AppearanceMode } from "../ui/theme";
import { SidebarItem, SectionLabel } from "../ui/Navigation";
import { IconButton } from "../ui/IconButton";
import { CommandTrigger } from "../ui/SearchField";
import { SidebarChrome, SidebarSearch, ShellFrame, ShellSidebar, ShellToolbar, WindowPanel } from "../ui/Chrome";
import { PlaceholderPage } from "../pages/PlaceholderPage";
import { CollectionPage } from "../pages/CollectionPage";
import { CollectionsBrowser, PlaylistSidebar } from "../pages/CollectionsBrowser";
import { LibraryPage } from "../pages/LibraryPage";
import { createCoreState } from "../state/core";
import { createNavigation, destinations, routeKey, type Route } from "./navigation";
import { Settings } from "./Settings";
import { CommandPalette } from "./CommandPalette";

export function AppShell(props: {
  initialMode?: AppearanceMode; initialOverlay?: "settings" | "command";
  initialSettingsCategory?: string; initialPluginLogin?: string; initialPluginResult?: unknown; initialRoute?: Route; injectedLibraryFolders?: string[]; initialNowPlaying?: boolean;
  onLyricChange?: (index: number | null) => void;
} = {}) {
  const appearance = createAppearance(props.initialMode);
  const nav = createNavigation(props.initialRoute);
  const core = createCoreState();
  const nowPlaying = signal(props.initialNowPlaying ?? false);
  const overlay = signal<"settings" | "command" | null>(props.initialOverlay ?? null);
  const showSettings = () => { nowPlaying.set(false); overlay.set("settings"); };
  const showCommands = () => { nowPlaying.set(false); overlay.set("command"); };
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
              selected={nav.current().kind === page.id || (page.id === "library" && ["album", "artist"].includes(nav.current().kind))}
              onPress={() => nav.navigate({ kind: page.id })} />)}
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
      <ShellToolbar />
      <View class="flex-1 min-h-0">
        {destinations.filter((page) => !["library", "favorites", "playlists"].includes(page.id)).map((page) => <PlaceholderPage page={page} active={nav.current().kind === page.id} />)}
        <If when={!!core()}>
          <CollectionPage core={core()!} id="__liked__" active={nav.current().kind === "favorites"} onDeleted={() => nav.navigate({ kind: "favorites" })} />
          <CollectionsBrowser core={core()!} active={nav.current().kind === "playlists"} navigate={nav.navigate} />
          <Each of={nav.history().filter((route, index, routes) => route.kind === "playlist"
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
            && routes.findIndex((other) => routeKey(route) === routeKey(other)) === index)}
            key={routeKey}>
            {(route) => <LibraryPage core={core()!}
              active={routeKey(nav.current()) === routeKey(route())}
              group={"ref" in route() ? (route() as Extract<Route, { ref: unknown }>).ref : undefined}
              groupKind={route().kind as "album" | "artist"} navigate={nav.navigate} />}
          </Each>
        </If>
        <If when={nav.current().kind === "library" && !core()}>
          <Text class="px-5 text-sm text-muted-foreground">{core.error() ? String(core.error()) : "正在加载曲库…"}</Text>
        </If>
      </View>
    </WindowPanel>
    <Settings open={overlay() === "settings"} onClose={closeOverlay} appearance={appearance} initialCategory={props.initialSettingsCategory} initialPluginLogin={props.initialPluginLogin} initialPluginResult={props.initialPluginResult} onLibrary={() => nav.navigate({kind: "library"})} />
    <If when={!!core()}>
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
