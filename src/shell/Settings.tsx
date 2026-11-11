import { createPlugins } from "../state/plugins";
import { SourcesSettings, PluginsSettings, DeveloperSettings } from "./PluginSettings";
import { createSettings } from "../state/settings";
import { PlaybackSettings } from "./PlaybackSettings";
import { LyricsSettings } from "./LyricsSettings";
import { ScrapeSettings } from "./ScrapeSettings";
import { DownloadSettings } from "./DownloadSettings";
import { AccountSettings } from "./AccountSettings";
import { SyncSettings } from "./SyncSettings";
import { StorageSettings } from "./StorageSettings";
import { ShortcutSettings } from "./ShortcutSettings";
import { OtherSettings } from "./Diagnostics";
import { Scroll, signal, Text, View } from "nsh";
import { type createAppearance } from "../ui/theme";
import { SidebarItem } from "../ui/Navigation";
import { SearchField } from "../ui/SearchField";
import { IconButton } from "../ui/IconButton";
import type { IconName } from "../ui/Icon";
import { Overlay } from "../ui/Surfaces";
import { AppearanceOptions } from "../ui/AppearanceOptions";
import { SidebarChrome, SidebarSearch, ShellFrame, ShellSidebar, ShellToolbar, WindowPanel } from "../ui/Chrome";

const categories = [
  { label: "账户", icon: "user" }, { label: "外观", icon: "palette" },
  { label: "播放", icon: "play" }, { label: "歌词", icon: "captions" },
  { label: "下载", icon: "download" }, { label: "存储", icon: "folder" },
  { label: "来源", icon: "radio" }, { label: "插件", icon: "plug" },
  { label: "同步", icon: "sync" }, { label: "快捷键", icon: "keyboard" },
  { label: "其他", icon: "settings" }, { label: "开发者", icon: "code" },
] as const satisfies readonly { label: string; icon: IconName }[];

export function Settings(props: { open: boolean; onClose: () => void; appearance: ReturnType<typeof createAppearance>; initialCategory?: string; initialShortcutConflict?: string; initialDownloadDirectory?: string; initialPluginLogin?: string; initialPluginResult?: unknown; initialLogs?: boolean; backupPath?: string; initialBackupDialog?: "export" | "import"; onLibrary?: () => void }) {
  const selected = signal(props.initialCategory ?? "外观");
  const settings = createSettings();
  const plugins = createPlugins();
  const query = signal("");
  return <Overlay open={props.open} onClose={props.onClose}>
    <ShellFrame>
      <ShellSidebar>
        <SidebarChrome />
        <SidebarSearch>
          <SearchField label="设置分类" value={query()} onChange={(value) => query.set(value)} placeholder="查找分类" sidebar />
        </SidebarSearch>
        <Scroll class="flex-1" contentClass="flex-col gap-(--navigation-row-gap) px-2 pb-2">
          {categories.map((category) => <View class="flex-col"
            style={{ display: category.label.includes(query()) ? "flex" : "none" }}>
            <SidebarItem icon={category.icon} label={category.label}
              selected={selected() === category.label} onPress={() => selected.set(category.label)} />
          </View>)}
        </Scroll>
      </ShellSidebar>
      <WindowPanel>
        <ShellToolbar testID="settings-heading">
          <IconButton icon="close" label="关闭设置" hint="关闭设置 · Esc" onPress={props.onClose} />
        </ShellToolbar>
        <Scroll class="flex-1" contentClass="flex-col px-(--content-inset) py-4">
          {/* Hidden checked Radio rings paint at the zero-size origin; mount only the active panel. */}
          <If when={selected() === "外观"}><View class="w-full max-w-2xl flex-col">
            <AppearanceOptions appearance={props.appearance} />
          </View></If>
          <If when={!!settings.value()}>
            <If when={selected() === "账户"}><View class="w-full max-w-2xl flex-col"><AccountSettings settings={settings} /></View></If>
            <If when={selected() === "同步"}><View class="w-full max-w-2xl flex-col"><SyncSettings settings={settings} /></View></If>
            <If when={selected() === "下载"}><View class="w-full max-w-2xl flex-col"><DownloadSettings initialDirectory={props.initialDownloadDirectory} settings={settings} /></View></If>
            <If when={selected() === "歌词"}><View class="w-full max-w-2xl"><LyricsSettings settings={settings} /></View></If>
            <View class="w-full max-w-2xl" style={{ display: selected() === "播放" ? "flex" : "none" }}><PlaybackSettings settings={settings} /></View>
            <View class="w-full max-w-2xl flex-col" style={{ display: selected() === "存储" ? "flex" : "none" }}>
              <StorageSettings settings={settings} onLibrary={() => { props.onClose(); props.onLibrary?.(); }} />
              <ScrapeSettings settings={settings} plugins={plugins} />
            </View>
          </If>
          <If when={!!settings.error()}><Text class="text-sm text-destructive">{settings.error()}</Text></If>
          <View class="w-full max-w-2xl" style={{ display: selected() === "来源" ? "flex" : "none" }}><SourcesSettings state={plugins} initialLogin={props.initialPluginLogin} /></View>
          <View class="w-full max-w-2xl" style={{ display: selected() === "插件" ? "flex" : "none" }}><PluginsSettings state={plugins} /></View>
          <View class="w-full max-w-5xl" style={{ display: selected() === "开发者" ? "flex" : "none" }}><DeveloperSettings state={plugins} initialResult={props.initialPluginResult} /></View>
          <If when={!!plugins.error() || !!plugins.core.error()}><Text class="text-sm text-destructive">{plugins.error() || String(plugins.core.error())}</Text></If>
          <If when={selected() === "快捷键" && !!settings.core()}><View class="w-full max-w-2xl"><ShortcutSettings core={settings.core()!} initialConflict={props.initialShortcutConflict} /></View></If>
          <If when={selected() === "其他" && !!settings.core()}><OtherSettings core={settings.core()!} initialLogs={props.initialLogs} backupPath={props.backupPath} initialBackupDialog={props.initialBackupDialog} /></If>
          <View class="flex-col" style={{ display: ["账户", "外观", "播放", "歌词", "下载", "存储", "来源", "插件", "开发者", "其他", "同步", "快捷键"].includes(selected()) ? "none" : "flex" }}>
            <Text class="text-sm text-muted-foreground">此分类将在对应功能接入后提供。</Text>
          </View>
        </Scroll>
      </WindowPanel>
    </ShellFrame>
  </Overlay>;
}
