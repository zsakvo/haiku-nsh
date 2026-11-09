import { Button as BaseButton } from "nsh/base";
import { Icon, type IconName } from "./Icon";
import { Tooltip } from "./Surfaces";
import { focus } from "./Button";
export function IconButton(props: {
  icon: IconName; label: string; hint?: string; disabled?: boolean;
  selected?: boolean; onPress: () => void;
}) {
  return <BaseButton accessibilityLabel={props.label} disabled={props.disabled}
    selected={props.selected} onPress={props.onPress}
    tooltip={<Tooltip label={props.hint ?? props.label} />}
    class={`size-8 items-center justify-center rounded-(--radius) hover:bg-accent active:bg-muted selected:bg-accent disabled:opacity-35 ${focus}`}>
    <Icon name={props.icon} small />
  </BaseButton>;
}
