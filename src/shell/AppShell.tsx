import { cleanup, effect, keymap, signal, Text, token, View } from "nsh";
import { Menu as DesktopMenu, Window } from "nsh/desktop";
import { Core } from "haiku/native";
import { createAppearance, modes, type AppearanceMode } from "../ui/theme";
import { SidebarItem, SectionLabel } from "../ui/Navigation";
import { IconButton } from "../ui/IconButton";
import { CommandTrigger } from "../ui/SearchField";
import { ChromeSpacer, ShellFrame, ShellSidebar, ShellToolbar, WindowPanel } from "../ui/Chrome";
import { PlaceholderPage } from "../pages/PlaceholderPage";
import { createNavigation, destinations } from "./navigation";
import { Settings } from "./Settings";
import { CommandPalette } from "./CommandPalette";

export function AppShell(props: { initialMode?: AppearanceMode; initialOverlay?: "settings" | "command" } = {}) {
  const appearance = createAppearance(props.initialMode);
  const nav = createNavigation();
  const overlay = signal<"settings" | "command" | null>(props.initialOverlay ?? null);
  const showSettings = () => overlay.set("settings");
  const showCommands = () => overlay.set("command");
  const closeOverlay = () => overlay.set(null);
  const goBack = () => { if (!overlay()) nav.back(); };
  const goForward = () => { if (!overlay()) nav.forward(); };
  const currentTitle = () => destinations.find((page) => page.id === nav.current().kind)?.label ?? "详情";
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
    if (id.startsWith("mode:")) appearance.mode.set(id.slice(5) as AppearanceMode);
    else {
      const page = destinations.find((entry) => entry.id === id);
      if (page) nav.navigate({ kind: page.id });
    }
    closeOverlay();
  }
  return <ShellFrame>
    <ShellSidebar>
      <ChromeSpacer />
      <View class="flex-1 min-h-0 gap-4 px-2 pb-4">
        {["发现", "音乐库"].map((group) => <View class="gap-0.5">
          <SectionLabel label={group} />
          {destinations.filter((page) => page.group === group).map((page) =>
            <SidebarItem testID={`nav-${page.id}`} icon={page.icon} label={page.label}
              selected={nav.current().kind === page.id} onPress={() => nav.navigate({ kind: page.id })} />)}
        </View>)}
        <View><SectionLabel label="歌单" /></View>
        <View class="flex-1" />
        <SidebarItem testID="nav-downloads" icon="download" label="下载"
          selected={nav.current().kind === "downloads"} onPress={() => nav.navigate({ kind: "downloads" })} />
      </View>
      <View class="h-24 shrink-0" />
    </ShellSidebar>
    <WindowPanel>
      <ShellToolbar>
        <IconButton icon="left" label="后退" hint={`后退 · ${keymap.label(shortcuts.back)}`}
          disabled={!nav.canBack()} onPress={goBack} />
        <IconButton icon="right" label="前进" hint={`前进 · ${keymap.label(shortcuts.forward)}`}
          disabled={!nav.canForward()} onPress={goForward} />
        <Text testID="page-title" role="header" class="ml-2 text-base font-semibold text-foreground">{currentTitle()}</Text>
        <View class="flex-1" />
        <CommandTrigger shortcut={keymap.label(shortcuts.commands)} onPress={showCommands} />
        <IconButton icon="settings" label="打开设置" hint={`设置 · ${keymap.label(shortcuts.settings)}`} onPress={showSettings} />
      </ShellToolbar>
      <View class="flex-1 min-h-0">
        {destinations.map((page) => <PlaceholderPage page={page} active={nav.current().kind === page.id} />)}
      </View>
    </WindowPanel>
    <Settings open={overlay() === "settings"} onClose={closeOverlay} appearance={appearance} />
    <CommandPalette open={overlay() === "command"} onClose={closeOverlay} onSelect={selectCommand}
      items={[
        ...destinations.map((page) => ({ id: page.id, label: `前往${page.label}`, group: "导航" })),
        { id: "settings", label: "打开设置…", group: "应用", shortcut: shortcuts.settings },
        ...modes.map((mode) => ({ id: `mode:${mode.id}`, label: `外观：${mode.label}`, group: "外观", checked: appearance.mode() === mode.id })),
      ]} />
  </ShellFrame>;
}
