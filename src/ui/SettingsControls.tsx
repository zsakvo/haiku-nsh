import { Text, View } from "nsh";
import { Select as BaseSelect, Slider as BaseSlider, Switch as BaseSwitch } from "nsh/base";
import { focus } from "./Button";
import { popupItem, popupSurface } from "./Surfaces";
export function SettingSwitch(props: { label: string; checked: boolean; onChange: (checked: boolean) => void }) {
  return <BaseSwitch checked={props.checked} onChange={(e) => props.onChange(e.checked)}
    accessibilityLabel={props.label} class={`min-h-8 flex-row items-center gap-3 ${focus}`}
    trackClass="w-9 rounded-full bg-muted p-0.5 checked:bg-primary disabled:opacity-40"
    thumbClass="size-4 rounded-full bg-background checked:translate-x-4">
    <Text class="text-sm text-foreground">{props.label}</Text>
  </BaseSwitch>;
}
export function SettingSlider(props: { label: string; value: number; min: number; max: number; step?: number; disabled?: boolean; onChange: (value: number) => void; testID?: string }) {
  return <View class="min-w-0 gap-1">
    <View class="flex-row justify-between gap-3"><Text class="text-xs text-muted-foreground">{props.label}</Text><Text class="text-xs text-foreground">{props.value.toFixed(1)}</Text></View>
    <BaseSlider testID={props.testID} accessibilityLabel={props.label} value={props.value} min={props.min} max={props.max} step={props.step ?? 0.5}
      disabled={props.disabled} onChange={(e) => { if (typeof e.value === "number") props.onChange(e.value); }}
      class="w-full disabled:opacity-40" trackClass="h-6 justify-center" indicatorClass="h-1 rounded-full bg-muted"
      rangeClass="rounded-full bg-foreground" thumbClass={`size-3 rounded-full border border-border bg-card hover:bg-accent active:bg-muted ${focus}`} />
  </View>;
}
export function SettingSelect(props: { label: string; value: string; items: { value: string; title: string }[]; onChange: (value: string) => void }) {
  return <View class="flex-row items-center justify-between gap-4">
    <Text class="text-sm text-foreground">{props.label}</Text>
    <BaseSelect accessibilityLabel={props.label} value={props.value} items={props.items} onChange={(e) => { if (e.value !== null) props.onChange(e.value); }}
      class="min-w-40" popupClass={`${popupSurface} p-1`} listClass="max-h-64" itemClass={popupItem}
      trigger={<View class={`min-h-8 flex-row items-center justify-between gap-3 rounded-(--radius) border border-input bg-card px-3 py-1.5 hover:bg-accent ${focus}`}>
        <Text class="text-sm text-foreground">{props.items.find((i) => i.value === props.value)?.title ?? "自定义"}</Text><Text class="text-xs text-muted-foreground">⌄</Text>
      </View>} />
  </View>;
}
