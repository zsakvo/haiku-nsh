import { createMotion, Image, Text, View, type JSX } from "nsh";
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

const skeletonRows = [
  { title: "w-48", artist: "w-24", album: "w-32" },
  { title: "w-60", artist: "w-20", album: "w-40" },
  { title: "w-40", artist: "w-28", album: "w-28" },
  { title: "w-56", artist: "w-24", album: "w-36" },
  { title: "w-44", artist: "w-20", album: "w-32" },
  { title: "w-52", artist: "w-28", album: "w-24" },
  { title: "w-36", artist: "w-24", album: "w-36" },
  { title: "w-48", artist: "w-20", album: "w-28" },
] as const;

/** Pulsing header + track-table skeleton matching DetailHeader and TrackList geometry. */
export function DetailLoadingSkeleton(props: { playlist?: boolean }) {
  const pulse = createMotion({ enter: "glow" });
  return (
    <View accessibilityLabel="正在加载详情" style={pulse.style} class="gap-4">
      <View class={`flex-row items-start gap-6 pb-4 ${props.playlist ? "pt-(--playlist-detail-top-gap)" : ""}`}>
        <View
          class={`shrink-0 rounded-(--detail-cover-radius) bg-muted ${
            props.playlist ? "size-(--playlist-detail-cover-size)" : "size-(--detail-cover-size)"
          }`}
        />
        <View
          class={`flex-1 min-w-0 justify-between gap-3 ${
            props.playlist ? "min-h-(--playlist-detail-cover-size)" : "min-h-(--detail-cover-size)"
          }`}
        >
          <View class="gap-3 pt-1">
            <View class="h-6 w-64 max-w-full rounded-(--radius) bg-muted" />
            <View class="h-3.5 w-80 max-w-full rounded-(--radius-inset) bg-muted" />
            <View class="h-3 w-36 rounded-(--radius-inset) bg-muted" />
          </View>
          <View class="flex-row items-center gap-3">
            <View class={`h-9 w-24 bg-muted ${props.playlist ? "rounded-full" : "rounded-(--radius)"}`} />
            <View class={`h-9 w-24 bg-muted ${props.playlist ? "rounded-full" : "rounded-(--radius)"}`} />
            <View class="size-9 rounded-(--radius) bg-muted" />
          </View>
        </View>
      </View>

      <View class="h-px bg-settings-divider" />

      <View class="gap-0.5 pt-1">
        {skeletonRows.map((row) => (
          <View class="h-(--detail-row-height) flex-row items-center gap-3 px-4">
            <View class="w-6 shrink-0 items-center">
              <View class="h-3 w-3 rounded-sm bg-muted" />
            </View>
            <View class="size-9 shrink-0 rounded-(--radius-inset) bg-muted" />
            <View class="flex-1 min-w-0 gap-2">
              <View class={`h-3.5 ${row.title} max-w-full rounded-(--radius-inset) bg-muted`} />
              <View class={`h-2.5 ${row.artist} max-w-full rounded-sm bg-muted`} />
            </View>
            <View class="flex-1 min-w-0">
              <View class={`h-3 ${row.album} max-w-full rounded-(--radius-inset) bg-muted`} />
            </View>
            <View class="w-(--detail-duration-width) shrink-0">
              <View class="h-3 w-9 rounded-sm bg-muted" />
            </View>
          </View>
        ))}
      </View>
    </View>
  );
}
