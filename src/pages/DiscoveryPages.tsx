import { cleanup, effect, List, resource, Scroll, signal, Text, View } from "nsh";
import type { Core } from "haiku/native";
import type { PluginSnapshot } from "../state/plugins";
import type { Route } from "../shell/navigation";
import { Button } from "../ui/Button";
import { CardShelf, ErrorState, Shelf, Skeleton } from "../ui/Discovery";
import { EmptyState } from "../ui/EmptyState";
import { SettingSelect } from "../ui/SettingsControls";
import { entityRoute, SourceGrid, SourceTracks } from "./SourceContent";
type HomeShelf={id:string;title:string;kind:string;items:any[];entries:{title:string;to:any}[];more?:any};
type HomeFeed={shelves:HomeShelf[];cached:boolean};
type Categories={groups:{title:string;items:{id:string;title:string}[]}[];sorts?:{id:string;title:string}[]};
function sources(snapshot:PluginSnapshot, capability:string) {
  return snapshot.sources.filter(s=>s.enabled).flatMap(s=>{
    const p=snapshot.packages.find(p=>p.id===s.pluginId&&p.enabled&&p.capabilities.includes(capability));
    return p?[{id:s.sourceId,title:s.displayName||p.name,caps:p.capabilities}]:[];
  });
}
type Props={core:Core;active:boolean;snapshot:PluginSnapshot;route:Route;navigate:(route:Route)=>void;settings:()=>void};
function useSource(props:Props,capability:string,key:string) {
  const selected=signal((props.core.settings.get() as any)[key]??"");
  const available=()=>sources(props.snapshot,capability);
  effect(()=>{if(!props.snapshot.packages.length)return;if(!available().some(s=>s.id===selected()))selected.set(available()[0]?.id??"");});
  async function select(id:string){selected.set(id);await props.core.settings.update({[key]:id});}
  return {selected,available,select};
}
function SourcePicker(props:{state:ReturnType<typeof useSource>;onChange?:(id:string)=>void}) {
  return <SettingSelect label="来源" value={props.state.selected()} items={props.state.available().map(s=>({value:s.id,title:s.title}))} onChange={v=>{props.onChange?.(v);void props.state.select(v);}} />;
}
function MissingSource(props:{settings:()=>void}) {return <EmptyState title="还没有可用来源" description="在设置中启用支持此功能的音乐来源。" action={{label:"管理来源…",onPress:props.settings}} />;}
export function HomePage(props:Props) {
  const state=useSource(props,"home","homeSourceId");
  const feeds=signal<Record<string,HomeFeed>>({}),errors=signal<Record<string,string>>({}),revision=signal(0);
  let version=0,lastRefresh=0,closed=false;cleanup(()=>{closed=true;version++;});
  const current=()=>feeds()[state.selected()];
  const identity=()=>props.active&&state.selected()?JSON.stringify([state.selected(),revision()]):undefined;
  const home=resource(identity,async key=>{
    const v=++version;const [source,refresh]=JSON.parse(key);errors.set({...errors(),[source]:""});
    const valid=()=>!closed&&v===version&&identity()===key;
    try {
      const force=refresh>lastRefresh;lastRefresh=refresh;
      const first=await props.core.sources.home(source,force) as HomeFeed;
      if(!valid())return;feeds.set({...feeds(),[source]:first});
      if(first.cached) {
        const fresh=await props.core.sources.home(source,true) as HomeFeed;
        if(valid())feeds.set({...feeds(),[source]:fresh});
      }
    }catch(e){if(valid())errors.set({...errors(),[source]:String(e)});}
  });
  const refresh=()=>revision.set(revision()+1);
  function link(link:any) {
    const ref=link.reference;
    if(ref.kind==="category"){props.navigate({kind:"categories",source:ref.source,category:ref.id});return;}
    if(ref.kind==="radio") {props.navigate({kind:"radio",source:ref.source});void props.core.player.startRadio(ref.source,ref.id);return;}
    props.navigate(entityRoute(link));
  }
  return <View testID="page-home" class="flex-1 min-h-0 pb-28" style={{display:props.active?"flex":"none",flexDirection:"column"}}>
    <View class="px-(--content-inset) pb-3 gap-3"><View class="flex-row items-center justify-between gap-3">
      <Text role="header" class="text-xl font-semibold text-foreground">推荐</Text><If when={!!state.available().length}><SourcePicker state={state} /></If>
    </View><View class="flex-row"><Button label="刷新推荐" size="sm" loading={home.loading()} disabled={!state.selected()} onPress={refresh} /></View></View>
    <If when={!state.available().length}><MissingSource settings={props.settings} /></If>
    <Scroll testID="home-scroll" class="flex-1" contentClass="flex-col px-(--content-inset) gap-6 pb-4">
      <If when={home.loading()&&!current()}><Skeleton label="正在加载推荐…" /></If>
      <If when={!!errors()[state.selected()]}><ErrorState cached={!!current()} message={errors()[state.selected()]} retry={refresh} /></If>
      <If when={!!current()&&!current().shelves.length}><Text class="text-sm text-muted-foreground">此来源暂无推荐内容</Text></If>
      <Each of={current()?.shelves??[]} key={s=>s.id}>{s=><Shelf id={`shelf-${s().id}`} title={s().title} more={s().more?()=>link(s().more):undefined}>
        <If when={s().kind==="tracks"}><SourceTracks core={props.core} tracks={s().items} active={props.active} fit /></If>
        <If when={s().kind==="entries"}><View class="flex-row flex-wrap gap-2"><Each of={s().entries} key={e=>JSON.stringify(e.to.reference)}>{e=><Button label={e().title} size="sm" onPress={()=>link(e().to)} />}</Each></View></If>
        <If when={s().kind!=="tracks"&&s().kind!=="entries"}><CardShelf items={s().items} open={e=>props.navigate(entityRoute(e))} /></If>
      </Shelf>}</Each>
    </Scroll>
  </View>;
}
export function CategoriesPage(props:Props) {
  const state=useSource(props,"categories.list","categorySourceId"), category=signal(""),title=signal(""),sort=signal("");
  const items=signal<any[]>([]),next=signal<string|null>(null),error=signal(""),busy=signal(false);
  let epoch=0,closed=false;cleanup(()=>{closed=true;epoch++;});
  effect(()=>{const r=props.route;if(r.kind==="categories"&&r.source){state.selected.set(r.source);category.set(r.category??"");title.set("");}});
  const groups=resource(()=>props.active&&state.selected()?state.selected():undefined,async s=>await props.core.sources.categories(s) as Categories);
  const categoryTitle=()=>groups()?.groups.flatMap(g=>g.items).find(c=>c.id===category())?.title || title() || "分类歌单";
  effect(()=>{const sorts=groups()?.sorts??[];if(!sorts.some((s:any)=>s.id===sort()))sort.set(sorts[0]?.id??"");});
  const identity=()=>props.active&&state.selected()&&category()?JSON.stringify([state.selected(),category(),sort()]):undefined;
  const result=resource(identity,async key=>{
    const v=++epoch;items.set([]);next.set(null);error.set("");busy.set(false);
    const [s,c,o]=JSON.parse(key);const page=await props.core.sources.categoryPlaylists(s,c,o||null,null) as any;
    if(!closed&&v===epoch&&identity()===key){items.set(page.items);next.set(page.next??null);}
  });
  async function more(){const key=identity(),v=epoch,cursor=next();if(!key||!cursor||busy())return;busy.set(true);error.set("");
    try{const [s,c,o]=JSON.parse(key);const page=await props.core.sources.categoryPlaylists(s,c,o||null,cursor) as any;
      if(!closed&&v===epoch&&identity()===key){items.set([...items(),...page.items]);next.set(page.next??null);}}
    catch(e){if(v===epoch)error.set(String(e));}finally{if(v===epoch)busy.set(false);}}
  function choose(id:string,name:string){category.set(id);title.set(name);}
  return <View testID="page-categories" class="flex-1 min-h-0 pb-28" style={{display:props.active?"flex":"none",flexDirection:"column"}}>
    <View class="px-(--content-inset) pb-3 flex-row items-center justify-between gap-3"><Text role="header" class="text-xl font-semibold text-foreground">分类</Text><If when={!!state.available().length}><SourcePicker state={state} onChange={()=>{category.set("");sort.set("");}} /></If></View>
    <If when={!state.available().length}><MissingSource settings={props.settings} /></If>
    <Scroll testID="categories-scroll" class="flex-1" contentClass="flex-col px-(--content-inset) gap-4 pb-4">
      <If when={!!groups.error()}><ErrorState message={String(groups.error())} retry={()=>groups.refetch()} /></If>
      <If when={!category()}>
        <If when={groups.loading()}><Skeleton label="正在加载分类…" /></If>
        <Each of={groups()?.groups??[]} key={g=>g.title}>{g=><Shelf id={`tags-${g().title}`} title={g().title}><View class="flex-row flex-wrap gap-2"><Each of={g().items} key={c=>c.id}>{c=><Button label={c().title} size="sm" onPress={()=>choose(c().id,c().title)} />}</Each></View></Shelf>}</Each>
      </If>
      <If when={!!category()}><View class="flex-row items-center gap-3"><Button label="全部分类" size="sm" variant="ghost" onPress={()=>category.set("")} /><Text testID="category-title" class="flex-1 text-lg font-semibold text-foreground">{categoryTitle()}</Text>
        <If when={!!groups()?.sorts?.length}><SettingSelect label="排序" value={sort()} items={(groups()?.sorts??[]).map((s:any)=>({value:s.id,title:s.title}))} onChange={v=>sort.set(v)} /></If>
      </View><If when={result.loading()}><Skeleton label="正在加载歌单…" /></If>
        <If when={!!result.error()||!!error()}><ErrorState message={error()||String(result.error())} retry={()=>result.refetch()} /></If>
        <If when={!!items().length}><SourceGrid items={items()} navigate={props.navigate} /></If>
        <If when={!result.loading()&&!result.error()&&!items().length}><Text class="text-sm text-muted-foreground">此分类暂无歌单</Text></If>
        <If when={!!next()}><View class="flex-row"><Button label="加载更多歌单" size="sm" loading={busy()} onPress={()=>void more()} /></View></If>
        <Text class="text-xs text-muted-foreground">{`已显示 ${items().length} 个歌单`}</Text>
      </If>
    </Scroll>
  </View>;
}
export function RadioPage(props:Props) {
  const state=useSource(props,"radio.next","radioSourceId"),error=signal(""),player=signal<any>(props.core.player.snapshot());
  effect(()=>{
    if(!props.active)return;
    player.set(props.core.player.snapshot());
    const stop=props.core.player.onState(s=>player.set(s));
    return stop;
  });
  effect(()=>{const r=props.route;if(r.kind==="radio"&&r.source)state.selected.set(r.source);});
  const channels=resource(()=>props.active&&state.available().some(p=>
    p.id===state.selected()&&p.caps.includes("radio.channels"))?state.selected():undefined,
    async s=>await props.core.sources.radioChannels(s) as any[]);
  async function start(id?:string){error.set("");try{await props.core.player.startRadio(state.selected(),id??null);}catch(e){error.set(String(e));}}
  const radio=()=>player().radio?.source===state.selected()?player().radio:null;
  return <View testID="page-radio" class="flex-1 min-h-0 pb-28" style={{display:props.active?"flex":"none",flexDirection:"column"}}>
    <View class="px-(--content-inset) pb-3 flex-row items-center justify-between gap-3"><Text role="header" class="text-xl font-semibold text-foreground">电台</Text><If when={!!state.available().length}><SourcePicker state={state} /></If></View>
    <If when={!state.available().length}><MissingSource settings={props.settings} /></If>
    <If when={!!state.available().length}><View class="px-(--content-inset) gap-3"><Shelf id="private-fm" title="为你播放"><View class="flex-row"><Button label="私人 FM" selected={!!radio()&&!radio().channel}
      variant={!!radio()&&!radio().channel?"primary":"secondary"} onPress={()=>void start()} /></View></Shelf>
      <If when={!!error()||!!channels.error()}><ErrorState message={error()||String(channels.error())} retry={()=>channels.refetch()} /></If>
      <If when={!!radio()?.error}><ErrorState message={radio().error} retry={()=>void start(radio().channel??undefined)} /></If>
      <If when={!!radio()}><Text class="text-sm text-muted-foreground">{radio().loading?"正在获取下一批曲目…":`持续播放 · 队列 ${player().tracks.length} 首`}</Text></If>
      <Text role="header" class="text-lg font-semibold text-foreground">频道</Text>
      <If when={channels.loading()}><Skeleton label="正在加载频道…" /></If>
    </View><List testID="radio-channels" each={channels()??[]} keyExtractor={c=>c.id} estimatedItemSize={44} class="flex-1">
      {c=><View class="px-(--content-inset) py-1 items-start"><Button label={c().title} selected={radio()?.channel===c().id} variant={radio()?.channel===c().id?"primary":"secondary"} onPress={()=>void start(c().id)} /></View>}
    </List></If>
  </View>;
}
