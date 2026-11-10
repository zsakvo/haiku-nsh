import { Text, View } from "nsh";
import { Input, ToggleGroup as BaseToggleGroup, Toggle } from "nsh/base";
import { focus } from "./Button";

/** Compact app-owned segmented control, including focus and selected states. */
export function ToggleGroup(props: {
  value: string; items: readonly { id: string; label: string }[]; onChange: (id: string) => void;
}) {
  return <BaseToggleGroup class="flex-row gap-1 rounded-(--radius) bg-muted p-1">
    {props.items.map((item) => <Toggle pressed={props.value === item.id} onChange={() => props.onChange(item.id)}
      class={`min-h-7 rounded-(--radius-inset) px-3 py-1 hover:bg-accent active:bg-muted aria-pressed:bg-card ${focus}`}>
      <Text class="text-sm text-foreground">{item.label}</Text>
    </Toggle>)}
  </BaseToggleGroup>;
}
export function Field(props: {
  label: string; value: string; placeholder?: string; onChange: (value: string) => void;
}) {
  return <View class="gap-1">
    <Text class="text-xs text-muted-foreground">{props.label}</Text>
    <Input accessibilityLabel={props.label} value={props.value} placeholder={props.placeholder}
      onChange={(event) => props.onChange(event.value)}
      class={`min-h-8 rounded-(--radius) border border-input bg-card px-3 py-1.5 text-sm text-foreground placeholder:text-muted-foreground ${focus}`} />
  </View>;
}
