import { Text } from "nsh";
import { Button as BaseButton } from "nsh/base";
import { Icon, type IconName } from "./Icon";
import { focus } from "./Button";
export function SidebarItem(props: { icon: IconName; label: string; selected: boolean; onPress: () => void; testID?: string }) {
  return <BaseButton testID={props.testID} selected={props.selected} onPress={props.onPress}
    class={`group flex-row items-center gap-2 rounded-(--radius) min-h-(--navigation-row-height) px-2 py-0.5 hover:bg-accent active:bg-muted selected:bg-selection ${focus}`}>
    <Icon name={props.icon} color={props.selected ? "primary" : "sidebar-foreground"} small />
    <Text class="flex-1 text-sm text-sidebar-foreground group-selected:text-selection-foreground group-selected:font-medium">{props.label}</Text>
  </BaseButton>;
}
export function SectionLabel(props: { label: string }) {
  return <Text role="header" class="px-2 pb-1 text-xs font-medium text-muted-foreground">{props.label}</Text>;
}
export function Kbd(props: { label: string }) {
  return <Text class="rounded-(--radius-inset) border border-border bg-muted px-1.5 py-0.5 text-xs font-mono text-muted-foreground">{props.label}</Text>;
}
