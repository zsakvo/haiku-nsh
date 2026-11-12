import { Image, Scroll, signal, Text, token, View } from "nsh";
import { Button as BaseButton } from "nsh/base";
import { Button, focus } from "./Button";
import { Icon } from "./Icon";
import { Tooltip } from "./Surfaces";
/** Shared content states keep recovery actions beside the failed source. */
export function ErrorState(props:{message:string;retry:()=>void;cached?:boolean}) {
  const expanded=signal(false);
  return <View class="gap-2 py-2"><Text style={{numberOfLines:expanded()?0:2}} class="text-sm text-destructive">{props.cached?`刷新失败，继续显示缓存：${props.message}`:props.message}</Text>
    <View class="flex-row gap-2"><Button label="重试" size="sm" onPress={props.retry} /><Button label={expanded()?"收起详情":"展开详情"} size="sm" variant="ghost" onPress={()=>expanded.set(!expanded())} /></View></View>;
}
export function Skeleton(props:{label:string}) {
  return <View accessibilityLabel={props.label} class="gap-2 py-2"><Text class="text-sm text-muted-foreground">{props.label}</Text><View class="h-24 rounded-(--radius-panel) bg-muted" /></View>;
}
export function Shelf(props:{id:string;title:string;more?:()=>void;children?:any}) {
  return <View testID={props.id} class="gap-2 shrink-0"><View class="flex-row items-center justify-between gap-3">
    <Text role="header" class="text-base font-semibold text-foreground">{props.title}</Text>
    <If when={!!props.more}><Button label={`更多${props.title}`} variant="ghost" size="sm" onPress={()=>props.more?.()} /></If>
  </View>{props.children}</View>;
}
export function Card(props:{title:string;subtitle?:string;cover?:string;onPress:()=>void;listening?:boolean}) {
  const coverClass=()=>props.listening?"size-(--listening-cover-size)":"size-28";
  return <BaseButton accessibilityLabel={props.title} onPress={props.onPress}
    class={`shrink-0 gap-2 items-start rounded-(--radius-panel) ${props.listening
      ?"w-(--listening-cover-size)"
      :"w-32 p-2 hover:bg-accent active:bg-muted"} ${focus}`}>
    <View class={`${coverClass()} rounded-(--radius) bg-muted overflow-hidden items-center justify-center`}>
      <If when={!!props.cover}><Image src={props.cover} class={coverClass()} style={{objectFit:"cover"}} /></If><If when={!props.cover}><Text class="text-muted-foreground">♪</Text></If>
    </View><Text class={`w-full text-sm ${props.listening?"text-muted-foreground":"text-foreground"}`}
      style={props.listening
        ?{fontSize:token("listening-card-title-size"),numberOfLines:2,textOverflow:"ellipsis"}
        :{numberOfLines:1}}>{props.title}</Text>
    <If when={!!props.subtitle}><Text class="text-xs text-muted-foreground" style={{numberOfLines:1}}>{props.subtitle}</Text></If>
  </BaseButton>;
}
export function CardShelf(props:{items:any[];open:(item:any)=>void;listening?:boolean}) {
  return <Scroll horizontal class={`${props.listening?"h-(--listening-card-height)":"h-44"} shrink-0`}
    contentClass={`flex-row ${props.listening?"gap-(--recommendation-cover-gap)":"gap-1 pb-2"}`}>
    <Each of={props.items} key={e=>JSON.stringify(e.reference)}>{e=><Card title={e().title} cover={e().cover} listening={props.listening} onPress={()=>props.open(e())} />}</Each>
  </Scroll>;
}
/** Cover navigation and playback are sibling controls with separate keyboard stops. */
export type RecommendationPreview={titles:string[];loading:boolean;error?:string};
export function RecommendationCard(props:{
  title:string;cover?:string;busy?:boolean;disabled?:boolean;open:()=>void;play:()=>void;
  preview?:RecommendationPreview;loadPreview:()=>void;
}) {
  const hovered=signal(false);
  const fade="transition-opacity duration-300 motion-reduce:transition-none";
  const visibility=()=>hovered()?"opacity-100":"opacity-0";
  return <View class="group relative w-(--recommendation-cover-width) h-(--recommendation-cover-height) shrink-0"
    onMouseEnter={()=>{hovered.set(true);props.loadPreview();}} onMouseLeave={()=>hovered.set(false)}>
    <BaseButton accessibilityLabel={props.title} onPress={props.open}
      class={`absolute inset-0 rounded-(--radius-inset) overflow-hidden bg-cover-placeholder active:opacity-85 ${focus}`}>
      <If when={!!props.cover}><Image src={props.cover} class="absolute inset-0" style={{objectFit:"cover"}} /></If>
      <If when={!props.cover}><View class="absolute inset-0 items-center justify-center"><Icon name="music" color="cover-foreground" /></View></If>
      <View class="absolute inset-x-0 top-0 h-24 pointer-events-none"
        style={{backgroundImage:`linear-gradient(to bottom, ${token("cover-shade")}, ${token("transparent")})`}} />
      <View class={`absolute inset-0 bg-cover-shade pointer-events-none ${fade} ${visibility()}`} />
      <View class="absolute top-3 left-3 right-3 pointer-events-none">
        <Text class="text-xs text-cover-foreground" style={{numberOfLines:2}}>{props.title}</Text>
        <View class={`mt-2 gap-1 ${fade} ${visibility()}`}>
          <If when={!props.preview||props.preview.loading}><Text class="text-xs text-cover-muted-foreground">正在加载曲目…</Text></If>
          <If when={!!props.preview?.error}><Text class="text-xs text-cover-muted-foreground">暂时无法加载曲目</Text></If>
          <If when={!!props.preview&&!props.preview.loading&&!props.preview.error&&!props.preview.titles.length}>
            <Text class="text-xs text-cover-muted-foreground">暂无曲目</Text>
          </If>
          <Each of={props.preview?.titles.slice(0,5)??[]} key={(_title,index)=>index}>{(title,index)=>
            <View class="flex-row items-center gap-1">
              <Text class="shrink-0 text-xs text-cover-muted-foreground">{`${index()+1}.`}</Text>
              <Text class="flex-1 min-w-0 text-xs text-cover-muted-foreground"
                style={{numberOfLines:1,textOverflow:"ellipsis"}}>{title()}</Text>
            </View>}
          </Each>
        </View>
      </View>
    </BaseButton>
    <BaseButton accessibilityLabel={`播放${props.title}`} onPress={props.play} disabled={props.disabled}
      tooltip={<Tooltip label={props.busy?"正在加载播放列表…":`播放${props.title}`} />}
      style={{pointerEvents:hovered()?"auto":"none"}}
      class={`absolute bottom-1.5 right-1.5 size-(--recommendation-play-size) rounded-full items-center justify-center bg-cover-play-background hover:bg-cover-play-hover active:bg-cover-play-active disabled:opacity-40 focus-visible:opacity-100 ${fade} ${visibility()} ${focus}`}>
      <Icon name={props.busy?"sync":"play"} color="cover-foreground" />
    </BaseButton>
  </View>;
}
