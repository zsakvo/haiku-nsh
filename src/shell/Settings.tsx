import { Scroll, signal, Text, token, View } from "nsh";
import { Radio, RadioGroup } from "nsh/base";
import { type AppearanceMode, brands, modes, type createAppearance } from "../ui/theme";
import { SidebarItem } from "../ui/Navigation";
import { SearchField } from "../ui/SearchField";
import { IconButton } from "../ui/IconButton";
import { Icon } from "../ui/Icon";
import { focus } from "../ui/Button";
import { Overlay } from "../ui/Surfaces";

const categories = ["账户", "外观", "播放", "歌词", "下载", "存储", "来源", "插件", "同步", "快捷键", "其他", "开发者"];
export function Settings(props: { open: boolean; onClose: () => void; appearance: ReturnType<typeof createAppearance> }) {
  const selected = signal("外观");
  const query = signal("");
  return <Overlay open={props.open} onClose={props.onClose}>
    <View class="flex-1 flex-row">
      <View class="w-55 shrink-0 border-r border-border bg-sidebar">
        <View class="h-14" desktop={{ windowDrag: true }} />
        <Text role="header" class="px-6 pb-4 text-lg font-semibold text-foreground">设置</Text>
        <View class="px-3 pb-3"><SearchField label="设置分类" value={query()} onChange={(value) => query.set(value)} placeholder="查找分类" /></View>
        <Scroll class="flex-1" contentClass="flex-col p-3">
          {categories.map((category) => <View class="flex-col" style={{ display: category.includes(query()) ? "flex" : "none" }}>
            <SidebarItem icon={category === "外观" ? "palette" : "settings"} label={category}
              selected={selected() === category} onPress={() => selected.set(category)} />
          </View>)}
        </Scroll>
      </View>
      <View class="flex-1 min-w-0">
        <View class="h-14 shrink-0 flex-row items-center justify-between px-6 border-b border-border" desktop={{ windowDrag: true }}>
          <Text testID="settings-heading" role="header" class="text-base font-semibold text-foreground">{selected()}</Text>
          <IconButton icon="close" label="关闭设置" hint="关闭设置 · Esc" onPress={props.onClose} />
        </View>
        <Scroll class="flex-1" contentClass="flex-col p-6">
          <View class="w-full max-w-2xl flex-col gap-6" style={{ display: selected() === "外观" ? "flex" : "none" }}>
            <View class="gap-2">
              <Text role="header" class="text-lg font-semibold text-foreground">让 Haiku 更像你</Text>
              <Text class="text-sm text-muted-foreground">选择舒服的明暗与色彩，立即预览变化。</Text>
            </View>
            <View class="gap-3">
              <Text role="header" class="text-sm font-semibold text-foreground">外观模式</Text>
              <RadioGroup class="flex-row gap-3">
                {modes.map((mode) => <Radio checked={props.appearance.mode() === mode.id}
                  onChange={() => props.appearance.mode.set(mode.id as AppearanceMode)}
                  class={`group flex-1 flex-row items-center gap-2 rounded-(--radius) border border-border bg-card px-3 py-3 hover:bg-accent active:bg-muted checked:border-ring checked:bg-selection ${focus}`}>
                  <View class="size-4 rounded-full border border-input items-center justify-center group-checked:border-ring">
                    <View class="size-2 rounded-full bg-primary opacity-0 group-checked:opacity-100" />
                  </View>
                  <Text class="text-sm text-foreground">{mode.label}</Text>
                </Radio>)}
              </RadioGroup>
            </View>
            <View class="h-px bg-border" />
            <View class="gap-3">
              <Text role="header" class="text-sm font-semibold text-foreground">品牌色</Text>
              <RadioGroup class="flex-row flex-wrap gap-2">
                {brands.map(([brand, label]) => <Radio accessibilityLabel={`品牌色：${label}`}
                  checked={props.appearance.brand() === brand} onChange={() => props.appearance.brand.set(brand)}
                  class={`group flex-row items-center gap-2 min-w-24 rounded-(--radius) border border-border px-3 py-2 hover:bg-accent active:bg-muted checked:border-ring checked:bg-selection ${focus}`}>
                  <View class="size-4 rounded-full" style={{ backgroundColor: token(`brand-${brand}`) }} />
                  <Text class="text-sm text-foreground">{label}</Text>
                  <View class="opacity-0 group-checked:opacity-100"><Icon name="check" small color="primary" /></View>
                </Radio>)}
              </RadioGroup>
              <Text class="text-xs text-muted-foreground">深色外观会调亮品牌色，保持文字与控件清晰。</Text>
            </View>
          </View>
          <View class="flex-col" style={{ display: selected() === "外观" ? "none" : "flex" }}>
            <Text class="text-sm text-muted-foreground">此分类将在对应功能接入后提供。</Text>
          </View>
        </Scroll>
      </View>
    </View>
  </Overlay>;
}
