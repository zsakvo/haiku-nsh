import { View } from "nsh";
import { Input } from "nsh/base";
import { Icon } from "./Icon";
export function SearchField(props: { value: string; label: string; placeholder?: string; onChange: (value: string) => void; autoFocus?: boolean }) {
  return <View class="flex-row items-center gap-2 rounded-(--radius) border border-input bg-card px-3 py-2 focus-within:border-ring">
    <Icon name="search" small color="muted-foreground" />
    <Input accessibilityLabel={props.label} value={props.value} placeholder={props.placeholder}
      onChange={(event) => props.onChange(event.value)} autoFocus={props.autoFocus}
      class="flex-1 min-w-0 text-sm text-foreground placeholder:text-muted-foreground" />
  </View>;
}
