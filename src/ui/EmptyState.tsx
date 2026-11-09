import { colorScheme, Image, Text, View } from "nsh";
import illustration from "../../assets/illustrations/listening.svg";
import darkIllustration from "../../assets/illustrations/listening-dark.svg";
import { Button } from "./Button";
export function EmptyState(props: { title: string; description: string; action?: { label: string; onPress: () => void } }) {
  return <View class="flex-1 items-center justify-center gap-2 px-(--content-inset) py-6">
    <Image src={colorScheme() === "dark" ? darkIllustration : illustration}
      class="h-36 w-50 mb-2" style={{ objectFit: "contain" }} />
    <Text role="header" class="text-xl font-semibold text-foreground">{props.title}</Text>
    <Text class="max-w-96 text-center text-sm text-muted-foreground">{props.description}</Text>
    {props.action && <Button label={props.action.label} onPress={props.action.onPress} />}
  </View>;
}
