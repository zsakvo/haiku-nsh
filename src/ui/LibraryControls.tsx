import { Text, token, View } from "nsh";
import { Input, ToggleGroup as BaseToggleGroup, Toggle } from "nsh/base";
import { focus } from "./Button";

/** Compact app-owned segmented control, including focus and selected states. */
export function ToggleGroup(props: {
  value: string; items: readonly { id: string; label: string }[]; onChange: (id: string) => void;
}) {
  return <BaseToggleGroup class="flex-row gap-0.5 rounded-(--radius-inset) bg-muted p-0.5">
    {props.items.map((item) => <Toggle pressed={props.value === item.id} onChange={() => props.onChange(item.id)}
      class={`min-h-5 rounded-sm px-2.5 py-0.5 active:opacity-70 aria-pressed:bg-card aria-pressed:shadow-sm ${focus}`}>
      <Text class="text-foreground" style={{ fontSize: token("control-text-size") }}>{item.label}</Text>
    </Toggle>)}
  </BaseToggleGroup>;
}
export function Field(props: {
  label: string; value: string; placeholder?: string; secret?: boolean; onChange: (value: string) => void;
}) {
  return <View class="gap-1">
    <Text class="text-xs text-muted-foreground">{props.label}</Text>
    <Input accessibilityLabel={props.label} value={props.value} placeholder={props.placeholder} masked={props.secret}
      onChange={(event) => props.onChange(event.value)}
      style={{ fontSize: token("control-text-size") }}
      class={`min-h-6 rounded-(--radius-inset) border border-input bg-card px-2 py-0.5 text-foreground placeholder:text-muted-foreground ${focus}`} />
  </View>;
}
