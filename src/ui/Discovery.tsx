import { Image, Scroll, signal, Text, View } from "nsh";
import { Button as BaseButton } from "nsh/base";
import { Button, focus } from "./Button";
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
    <Text role="header" class="text-lg font-semibold text-foreground">{props.title}</Text>
    <If when={!!props.more}><Button label={`更多${props.title}`} variant="ghost" size="sm" onPress={()=>props.more?.()} /></If>
  </View>{props.children}</View>;
}
export function Card(props:{title:string;subtitle?:string;cover?:string;onPress:()=>void}) {
  return <BaseButton accessibilityLabel={props.title} onPress={props.onPress}
    class={`w-32 shrink-0 gap-2 items-start rounded-(--radius-panel) p-2 hover:bg-accent active:bg-muted ${focus}`}>
    <View class="size-28 rounded-(--radius) bg-muted overflow-hidden items-center justify-center">
      <If when={!!props.cover}><Image src={props.cover} class="size-28" /></If><If when={!props.cover}><Text class="text-muted-foreground">♪</Text></If>
    </View><Text class="text-sm text-foreground" style={{numberOfLines:1}}>{props.title}</Text>
    <If when={!!props.subtitle}><Text class="text-xs text-muted-foreground" style={{numberOfLines:1}}>{props.subtitle}</Text></If>
  </BaseButton>;
}
export function CardShelf(props:{items:any[];open:(item:any)=>void}) {
  return <Scroll horizontal class="h-44 shrink-0" contentClass="flex-row gap-1 pb-2">
    <Each of={props.items} key={e=>JSON.stringify(e.reference)}>{e=><Card title={e().title} cover={e().cover} onPress={()=>props.open(e())} />}</Each>
  </Scroll>;
}
