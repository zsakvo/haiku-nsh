import { Image, Text, View } from "nsh";
import illustration from "../../assets/illustrations/listening.svg";
import { Button } from "./Button";
export function EmptyState(props: { title: string; description: string; action?: { label: string; onPress: () => void } }) {
  return <View class="flex-1 items-center justify-center gap-3 p-8">
    <Image src={illustration} class="h-40 w-56 mb-3" style={{ objectFit: "contain" }} />
    <Text role="header" class="text-xl font-semibold text-foreground">{props.title}</Text>
    <Text class="max-w-96 text-center text-sm text-muted-foreground">{props.description}</Text>
    {props.action && <Button label={props.action.label} onPress={props.action.onPress} />}
  </View>;
}
