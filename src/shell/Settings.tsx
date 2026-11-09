import { Scroll, signal, Text, View } from "nsh";
import { type createAppearance } from "../ui/theme";
import { SidebarItem } from "../ui/Navigation";
import { SearchField } from "../ui/SearchField";
import { IconButton } from "../ui/IconButton";
import type { IconName } from "../ui/Icon";
import { Overlay } from "../ui/Surfaces";
import { AppearanceOptions } from "../ui/AppearanceOptions";
import { ChromeSpacer, ShellFrame, ShellSidebar, ShellToolbar, WindowPanel } from "../ui/Chrome";

const categories = [
  { label: "账户", icon: "user" }, { label: "外观", icon: "palette" },
  { label: "播放", icon: "play" }, { label: "歌词", icon: "captions" },
  { label: "下载", icon: "download" }, { label: "存储", icon: "folder" },
  { label: "来源", icon: "radio" }, { label: "插件", icon: "plug" },
  { label: "同步", icon: "sync" }, { label: "快捷键", icon: "keyboard" },
  { label: "其他", icon: "settings" }, { label: "开发者", icon: "code" },
] as const satisfies readonly { label: string; icon: IconName }[];

export function Settings(props: { open: boolean; onClose: () => void; appearance: ReturnType<typeof createAppearance> }) {
  const selected = signal("外观");
  const query = signal("");
  return <Overlay open={props.open} onClose={props.onClose}>
    <ShellFrame>
      <ShellSidebar>
        <ChromeSpacer />
        <View class="px-2 pb-2">
          <SearchField label="设置分类" value={query()} onChange={(value) => query.set(value)} placeholder="查找分类" />
        </View>
        <Scroll class="flex-1" contentClass="flex-col gap-0.5 px-2 pb-2">
          {categories.map((category) => <View class="flex-col"
            style={{ display: category.label.includes(query()) ? "flex" : "none" }}>
            <SidebarItem icon={category.icon} label={category.label}
              selected={selected() === category.label} onPress={() => selected.set(category.label)} />
          </View>)}
        </Scroll>
      </ShellSidebar>
      <WindowPanel>
        <ShellToolbar>
          <Text testID="settings-heading" role="header" class="flex-1 text-base font-semibold text-foreground">{selected()}</Text>
          <IconButton icon="close" label="关闭设置" hint="关闭设置 · Esc" onPress={props.onClose} />
        </ShellToolbar>
        <Scroll class="flex-1" contentClass="flex-col px-(--content-inset) py-4">
          <View class="w-full max-w-2xl flex-col" style={{ display: selected() === "外观" ? "flex" : "none" }}>
            <AppearanceOptions appearance={props.appearance} />
          </View>
          <View class="flex-col" style={{ display: selected() === "外观" ? "none" : "flex" }}>
            <Text class="text-sm text-muted-foreground">此分类将在对应功能接入后提供。</Text>
          </View>
        </Scroll>
      </WindowPanel>
    </ShellFrame>
  </Overlay>;
}
