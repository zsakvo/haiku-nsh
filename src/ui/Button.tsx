import { Text } from "nsh";
import { Button as BaseButton } from "nsh/base";

/** The M0 screen's neutral action, retaining base keyboard/focus behavior. */
export function Button(props: {
  label: string;
  disabled?: boolean;
  onPress: () => void;
}) {
  return (
    <BaseButton
      disabled={props.disabled}
      onPress={props.onPress}
      class="items-center justify-center rounded-(--radius) border border-border bg-background px-4 py-2 hover:bg-accent active:bg-muted focus:outline-2 focus:outline-ring disabled:opacity-50"
    >
      <Text class="text-base text-foreground">{props.label}</Text>
    </BaseButton>
  );
}
