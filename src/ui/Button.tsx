import { Text } from "nsh";
import { Button as BaseButton } from "nsh/base";
export type ButtonVariant = "primary" | "secondary" | "ghost" | "destructive";
const variants: Record<ButtonVariant, string> = {
  primary: "bg-primary hover:bg-primary/90 active:bg-primary/80",
  secondary: "border border-border bg-secondary hover:bg-accent active:bg-muted",
  ghost: "hover:bg-accent active:bg-muted selected:bg-accent",
  destructive: "bg-destructive hover:bg-destructive/90 active:bg-destructive/80",
};
const foregrounds: Record<ButtonVariant, string> = {
  primary: "text-primary-foreground", secondary: "text-secondary-foreground",
  ghost: "text-foreground", destructive: "text-destructive-foreground",
};
export const focus = "focus-visible:outline-2 focus-visible:outline-ring";
export function Button(props: {
  label: string; variant?: ButtonVariant; size?: "sm" | "md"; disabled?: boolean;
  loading?: boolean; selected?: boolean; onPress: () => void; testID?: string;
  accessibilityLabel?: string;
}) {
  return <BaseButton testID={props.testID} disabled={props.disabled || props.loading}
    accessibilityLabel={props.accessibilityLabel}
    selected={props.selected} onPress={props.onPress}
    class={`items-center justify-center rounded-(--radius) ${focus} disabled:opacity-40
      ${props.size === "sm" ? "min-h-7 px-2 py-1" : "min-h-8 px-3 py-1.5"}
      ${variants[props.variant ?? "secondary"]}`}>
    <Text class={`text-sm ${foregrounds[props.variant ?? "secondary"]}`}>
      {props.loading ? `${props.label}…` : props.label}
    </Text>
  </BaseButton>;
}
