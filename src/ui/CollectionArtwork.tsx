import { Image, Text, View } from "nsh";

/** Four stable cover slots; absent artwork uses the app's neutral music placeholder. */
export function CollectionArtwork(props: { covers: (string | null)[]; small?: boolean }) {
  const size = () => props.small ? "size-12" : "size-32";
  const tile = () => props.small ? "size-6" : "size-16";
  return <View class={`${size()} shrink-0 flex-row flex-wrap overflow-hidden rounded-(--radius) bg-muted`}>
    {[0, 1, 2, 3].map((index) => <View class={`${tile()} items-center justify-center bg-muted`}>
      <If when={!!props.covers[index]}>
        <Image src={props.covers[index]!} class={tile()} />
      </If>
      <If when={!props.covers[index]}><Text class="text-muted-foreground">♪</Text></If>
    </View>)}
  </View>;
}
