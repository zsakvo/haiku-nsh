import { Text, token, View } from "nsh";
import { Button, Input } from "nsh/base";
import { Icon } from "./Icon";
import { Kbd } from "./Navigation";
import { Tooltip } from "./Surfaces";
import { focus } from "./Button";
export function SearchField(props: { value: string; label: string; placeholder?: string; onChange: (value: string) => void; onSubmit?: () => void; autoFocus?: boolean; sidebar?: boolean }) {
  return <View class="min-h-(--compact-control-height) flex-row items-center gap-2 rounded-(--radius) border border-input bg-card px-2 py-1 focus-within:border-ring">
    <Icon name="search" small sidebar={props.sidebar} color="muted-foreground" />
    <Input accessibilityLabel={props.label} value={props.value} placeholder={props.placeholder}
      onChange={(event) => props.onChange(event.value)} onSubmit={props.onSubmit} autoFocus={props.autoFocus}
      style={props.sidebar ? { fontSize: token(props.value ? "sidebar-text-size" : "sidebar-hint-size") } : {}}
      class="flex-1 min-w-0 text-sm text-foreground placeholder:text-muted-foreground" />
  </View>;
}
export function CommandTrigger(props: { shortcut: string; onPress: () => void; class?: string }) {
  return <Button accessibilityLabel="打开命令面板" onPress={props.onPress}
    tooltip={<Tooltip label={`命令面板 · ${props.shortcut}`} />}
    class={`h-(--navigation-control-height) min-w-(--command-entry-min-width) flex-row items-center gap-2 rounded-(--radius) border border-border bg-muted px-3 hover:bg-accent active:bg-muted ${focus} ${props.class ?? ""}`}>
    <Icon name="search" small color="muted-foreground" />
    <Text class="flex-1 text-sm text-muted-foreground" style={{ numberOfLines: 1 }}>搜索或前往…</Text><Kbd sidebar label={props.shortcut} />
  </Button>;
}
