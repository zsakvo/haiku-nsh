import { Text, View } from "nsh";
import { Button, Input } from "nsh/base";
import { Icon } from "./Icon";
import { Kbd } from "./Navigation";
import { Tooltip } from "./Surfaces";
import { focus } from "./Button";
export function SearchField(props: { value: string; label: string; placeholder?: string; onChange: (value: string) => void; autoFocus?: boolean; compactPlaceholder?: boolean }) {
  return <View class="min-h-(--compact-control-height) flex-row items-center gap-2 rounded-(--radius) border border-input bg-card px-2 py-1 focus-within:border-ring">
    <Icon name="search" small color="muted-foreground" />
    <Input accessibilityLabel={props.label} value={props.value} placeholder={props.placeholder}
      onChange={(event) => props.onChange(event.value)} autoFocus={props.autoFocus}
      class={`flex-1 min-w-0 ${props.compactPlaceholder && !props.value ? "text-xs" : "text-sm"} text-foreground placeholder:text-muted-foreground`} />
  </View>;
}
export function CommandTrigger(props: { shortcut: string; onPress: () => void }) {
  return <Button accessibilityLabel="打开命令面板" onPress={props.onPress}
    tooltip={<Tooltip label={`命令面板 · ${props.shortcut}`} />}
    class={`min-h-(--compact-control-height) min-w-(--command-entry-min-width) flex-row items-center gap-2 rounded-full border border-border bg-card px-3 py-0.5 hover:bg-accent active:bg-muted ${focus}`}>
    <Icon name="search" small color="muted-foreground" />
    <Text class="flex-1 text-xs text-muted-foreground">前往…</Text><Kbd label={props.shortcut} />
  </Button>;
}
