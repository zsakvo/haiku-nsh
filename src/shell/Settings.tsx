import { createSettings } from "../state/settings";
import { PlaybackSettings } from "./PlaybackSettings";
import { StorageSettings } from "./StorageSettings";
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

export function Settings(props: { open: boolean; onClose: () => void; appearance: ReturnType<typeof createAppearance>; initialCategory?: string; onLibrary?: () => void }) {
  const selected = signal(props.initialCategory ?? "外观");
  const settings = createSettings();
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
          <View class="w-full max-w-2xl flex-col" style={{ display: selected() === "外观" ? "flex" : "none" }}>
            <AppearanceOptions appearance={props.appearance} />
          </View>
          <If when={!!settings.value()}>
            <View class="w-full max-w-2xl" style={{ display: selected() === "播放" ? "flex" : "none" }}><PlaybackSettings settings={settings} /></View>
            <View class="w-full max-w-2xl" style={{ display: selected() === "存储" ? "flex" : "none" }}><StorageSettings settings={settings} onLibrary={() => { props.onClose(); props.onLibrary?.(); }} /></View>
          </If>
          <If when={!!settings.error()}><Text class="text-sm text-destructive">{settings.error()}</Text></If>
          <View class="flex-col" style={{ display: ["外观", "播放", "存储"].includes(selected()) ? "none" : "flex" }}>
            <Text class="text-sm text-muted-foreground">此分类将在对应功能接入后提供。</Text>
          </View>
        </Scroll>
      </WindowPanel>
    </ShellFrame>
  </Overlay>;
}
