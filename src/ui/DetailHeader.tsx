import { Image, Text, View, type JSX } from "nsh";
import { Button as BaseButton } from "nsh/base";
import { focus } from "./Button";
import { Icon, type IconName } from "./Icon";

/** Shared playlist geometry follows the reference desktop detail page. */
export function DetailHeader(props: {
  title: string; cover?: string; description?: string; metadata?: string;
  playlist?: boolean; children?: JSX.Element;
}) {
  return <View testID="detail-header" class={`flex-row items-start gap-6 pb-4 ${props.playlist ? "pt-(--playlist-detail-top-gap)" : ""}`}>
    <View testID="detail-cover" class={`shrink-0 rounded-(--detail-cover-radius) overflow-hidden bg-muted items-center justify-center
      ${props.playlist ? "size-(--playlist-detail-cover-size)" : "size-(--detail-cover-size)"}`}>
      <If when={!!props.cover}><Image src={props.cover!} class="size-full" style={{objectFit:"cover"}} /></If>
      <If when={!props.cover}><Icon name="music" color="muted-foreground" /></If>
    </View>
    <View class={`flex-1 min-w-0 gap-3 justify-between ${props.playlist ? "min-h-(--playlist-detail-cover-size)" : "min-h-(--detail-cover-size)"}`}>
      <View class="gap-3">
      <Text role="header" class="text-xl font-medium text-foreground" style={{numberOfLines:2}}>{props.title}</Text>
      <If when={!!props.description}><Text class="text-(--detail-description-size) text-muted-foreground"
        style={{numberOfLines:2,textOverflow:"ellipsis"}}>{props.description}</Text></If>
      <If when={!!props.metadata}><Text class="text-xs text-muted-foreground" style={{numberOfLines:2}}>{props.metadata}</Text></If>
      </View>
      <View class="flex-row flex-wrap gap-3 items-center">{props.children}</View>
    </View>
  </View>;
}

export function DetailAction(props: {
  label: string; icon: IconName; primary?: boolean; pill?: boolean; disabled?: boolean; onPress:()=>void;
}) {
  return <BaseButton accessibilityLabel={props.label} disabled={props.disabled} onPress={props.onPress}
    class={`flex-row items-center justify-center gap-2 min-h-9 px-3.5 py-2 disabled:opacity-40 ${focus}
      ${props.pill ? "rounded-full" : "rounded-(--radius)"}
      ${props.primary?"bg-primary active:bg-primary/80":"bg-secondary active:bg-muted"}`}>
    <Icon name={props.icon} small color={props.primary?"primary-foreground":"foreground"} />
    <Text class={`text-(--detail-description-size) font-medium ${props.primary?"text-primary-foreground":"text-foreground"}`}>{props.label}</Text>
  </BaseButton>;
}
