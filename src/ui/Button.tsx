import { Text, token } from "nsh";
import { Button as BaseButton } from "nsh/base";
export type ButtonVariant = "primary" | "secondary" | "ghost" | "destructive";
const variants: Record<ButtonVariant, string> = {
  primary: "bg-primary active:bg-primary/80",
  secondary: "border border-border bg-card shadow-sm active:bg-muted",
  ghost: "active:bg-muted selected:bg-accent",
  destructive: "bg-destructive active:bg-destructive/80",
};
const foregrounds: Record<ButtonVariant, string> = {
  primary: "text-primary-foreground", secondary: "text-secondary-foreground",
  ghost: "text-foreground", destructive: "text-destructive-foreground",
};
export const focus = "focus-visible:outline-2 focus-visible:outline-ring";
export function Button(props: {
  label: string; variant?: ButtonVariant; size?: "sm" | "md"; disabled?: boolean;
  loading?: boolean; selected?: boolean; onPress: () => void; testID?: string;
  accessibilityLabel?: string; class?: string;
}) {
  return <BaseButton testID={props.testID} disabled={props.disabled || props.loading}
    accessibilityLabel={props.accessibilityLabel}
    selected={props.selected} onPress={props.onPress}
    class={`items-center justify-center rounded-(--radius-inset) ${focus} disabled:opacity-40
      ${props.size === "sm" ? "min-h-5 px-2 py-0.5" : "min-h-6 px-3 py-0.5"}
      ${variants[props.variant ?? "secondary"]} ${props.class??""}`}>
    <Text class={foregrounds[props.variant ?? "secondary"]}
      style={{ fontSize: token(props.size === "sm" ? "control-text-sm-size" : "control-text-size") }}>
      {props.loading ? `${props.label}…` : props.label}
    </Text>
  </BaseButton>;
}
