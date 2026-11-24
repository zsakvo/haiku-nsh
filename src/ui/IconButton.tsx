import { Button as BaseButton } from "nsh/base";
import { Icon, type IconName } from "./Icon";
import { Tooltip } from "./Surfaces";
import { focus } from "./Button";
import { NavigationControlBackdrop } from "./Chrome";
export function IconButton(props: {
  icon: IconName; label: string; hint?: string; disabled?: boolean;
  selected?: boolean; onPress: () => void;
  sidebar?: boolean; testID?: string;
  navigation?: boolean;
}) {
  return <BaseButton testID={props.testID} accessibilityLabel={props.label} disabled={props.disabled}
    selected={props.selected} onPress={props.onPress}
    tooltip={<Tooltip label={props.hint ?? props.label} />}
    class={`${props.navigation
      ? "group relative size-(--navigation-control-height) overflow-hidden rounded-(--radius) border border-source-chip-border"
      : "size-(--compact-control-height) rounded-(--radius) hover:bg-accent active:bg-muted selected:bg-accent"} items-center justify-center disabled:opacity-35 ${focus}`}>
    <If when={!!props.navigation}><NavigationControlBackdrop /></If>
    <Icon name={props.icon} small sidebar={props.sidebar} navigation={props.navigation} color={props.selected ? "primary" : props.sidebar ? "sidebar-foreground" : "foreground"} />
  </BaseButton>;
}
