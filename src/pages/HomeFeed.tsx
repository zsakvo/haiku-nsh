import { effect, Image, signal, Text, token, View } from "nsh";
import { Button as BaseButton } from "nsh/base";
import type { Core } from "haiku/native";
import { focus } from "../ui/Button";
import { Icon } from "../ui/Icon";
import type { HomeTrack } from "./HomeTracks";

/** One waterfall item in feed order: a song, or a collection card (playlist, album, entry…). */
export type FeedItem = { key: string; kind: string; item: any };
export const feedGap = 16;

/** A recycled grid row. Network page boundaries never determine visual rows. */
export function FeedRow(props: { core: Core; items: FeedItem[]; columns: number; open: (item: FeedItem) => void }) {
  const error = signal("");
  effect(()=>{props.items;error.set("");});
  const tracks = () => props.items.filter(i => i.kind === "tracks").map(i => i.item as HomeTrack);
  async function play(track: HomeTrack) {
    error.set("");
    const list = tracks();
    try { await props.core.player.play(list, list.indexOf(track)); } catch (e) { error.set(String(e)); }
  }
  return <View testID="home-feed" class="w-full gap-2">
    <View class="flex-row items-start" style={{ gap: feedGap }}>
      <Each of={Array.from({length:props.columns},(_,index)=>index)}>{column =>
        <View class="flex-1 min-w-0">
          <If when={!!props.items[column()]}><FeedCard
            title={props.items[column()].item.title} cover={props.items[column()].item.cover}
            track={props.items[column()].kind==="tracks"?props.items[column()].item:undefined}
            onPress={()=>{const entry=props.items[column()];if(entry.kind==="tracks")void play(entry.item);else props.open(entry);}} /></If>
        </View>}
      </Each>
    </View>
    <If when={!!error()}><Text class="text-sm text-destructive">{error()}</Text></If>
  </View>;
}

function FeedCard(props: { title: string; cover?: string; track?: HomeTrack; onPress: () => void }) {
  const artists=()=>props.track?.artists.map(a=>typeof a==="string"?a:a.name).join(" / ")||"未知艺术家";
  return <BaseButton accessibilityLabel={props.track?`播放${props.title}`:props.title} onPress={props.onPress}
    class={`group gap-2 items-stretch rounded-(--radius-panel) active:opacity-85 ${focus}`}>
    <View class="w-full aspect-square rounded-(--radius) bg-muted overflow-hidden items-center justify-center">
      <If when={!!props.cover}><Image src={props.cover!} class="absolute inset-0" style={{ objectFit: "cover" }} /></If>
      <If when={!props.cover}><Icon name="music" color="muted-foreground" /></If>
      <If when={!!props.track}><View class="absolute inset-0 items-center justify-center bg-cover-shade opacity-0 group-hover:opacity-100 group-focus-visible:opacity-100">
        <Icon name="play" color="cover-foreground" />
      </View></If>
    </View>
    <Text class="w-full text-muted-foreground"
      style={{ fontSize: token("listening-card-title-size"), numberOfLines: 2, textOverflow: "ellipsis" }}>{props.title}</Text>
    <If when={!!props.track}><Text class="w-full text-xs text-muted-foreground"
      style={{numberOfLines:1,textOverflow:"ellipsis"}}>{artists()}</Text></If>
  </BaseButton>;
}
