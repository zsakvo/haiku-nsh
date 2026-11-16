import { Button as BaseButton } from "nsh/base";
import { Icon, type IconName } from "./Icon";
import { Tooltip } from "./Surfaces";
import { focus } from "./Button";
export function IconButton(props: {
  icon: IconName; label: string; hint?: string; disabled?: boolean;
  selected?: boolean; onPress: () => void;
  sidebar?: boolean; testID?: string;
  navigation?: boolean;
}) {
  return <BaseButton testID={props.testID} accessibilityLabel={props.label} disabled={props.disabled}
    selected={props.selected} onPress={props.onPress}
    tooltip={<Tooltip label={props.hint ?? props.label} />}
    class={`${props.navigation ? "size-(--navigation-control-height) rounded-(--radius) border border-border bg-card" : "size-(--compact-control-height) rounded-(--radius)"} items-center justify-center hover:bg-accent active:bg-muted selected:bg-accent disabled:opacity-35 ${focus}`}>
    <Icon name={props.icon} small sidebar={props.sidebar} navigation={props.navigation} color={props.selected ? "primary" : props.sidebar ? "sidebar-foreground" : "foreground"} />
  </BaseButton>;
}
