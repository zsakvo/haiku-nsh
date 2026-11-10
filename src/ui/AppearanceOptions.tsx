import { Text, token, View } from "nsh";
import { Radio, RadioGroup } from "nsh/base";
import { brands, modes, type createAppearance } from "./theme";
import { Icon } from "./Icon";
import { focus } from "./Button";

/** Appearance controls share density, cards, and fixed selection slots. */
export function AppearanceOptions(props: { appearance: ReturnType<typeof createAppearance> }) {
  return <View class="gap-4">
    <If when={!!props.appearance.error()}><Text class="text-sm text-destructive">{props.appearance.error()}</Text></If>
    <View class="gap-1">
      <Text role="header" class="text-lg font-semibold text-foreground">让 Haiku 更像你</Text>
      <Text class="text-sm text-muted-foreground">选择舒服的明暗与色彩，立即预览变化。</Text>
    </View>
    <View class="gap-2">
      <Text role="header" class="text-sm font-medium text-foreground">外观模式</Text>
      <RadioGroup class="flex-row gap-2">
        {modes.map((mode) => <Radio checked={props.appearance.mode() === mode.id}
          onChange={() => void props.appearance.setMode(mode.id)}
          class={`group flex-1 flex-row items-center gap-2 rounded-(--radius-panel) border border-border bg-card px-3 py-2 hover:bg-accent active:bg-muted checked:ring-2 checked:ring-ring checked:bg-selection ${focus}`}>
          <View class="size-4 rounded-full border border-input items-center justify-center group-checked:border-ring">
            <View class="size-2 rounded-full bg-primary opacity-0 group-checked:opacity-100" />
          </View>
          <Text class="text-sm text-foreground">{mode.label}</Text>
        </Radio>)}
      </RadioGroup>
    </View>
    <View class="gap-2">
      <Text role="header" class="text-sm font-medium text-foreground">品牌色</Text>
      <View class="rounded-(--radius-panel) border border-border bg-card p-3">
        <RadioGroup class="grid gap-2"
          style={{ gridTemplateColumns: `repeat(auto-fit, minmax(${token("brand-option-min-width")}px, 1fr))` }}>
          {brands.map(([brand, label]) => <Radio accessibilityLabel={`品牌色：${label}`}
            checked={props.appearance.brand() === brand} onChange={() => void props.appearance.setBrand(brand)}
            class={`group min-w-0 flex-row items-center gap-2 rounded-(--radius) border border-border px-2 py-1.5 hover:bg-accent active:bg-muted checked:ring-2 checked:ring-ring checked:bg-selection ${focus}`}>
            <View class="size-4 rounded-full" style={{ backgroundColor: token(`brand-${brand}`) }} />
            <Text class="flex-1 text-sm text-foreground">{label}</Text>
            <View class="size-4 opacity-0 group-checked:opacity-100"><Icon name="check" small color="primary" /></View>
          </Radio>)}
        </RadioGroup>
      </View>
      <Text class="text-xs text-muted-foreground">深色外观会调亮品牌色，保持文字与控件清晰。</Text>
    </View>
  </View>;
}
