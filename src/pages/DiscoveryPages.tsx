import { cleanup, computed, effect, List, resource, Scroll, signal, Text, token, untracked, View } from "nsh";
import type { Core } from "haiku/native";
import type { PluginSnapshot } from "../state/plugins";
import type { Route } from "../shell/navigation";
import { Button } from "../ui/Button";
import { CardShelf, ErrorState, Shelf, Skeleton, Spinner } from "../ui/Discovery";
import { EmptyState } from "../ui/EmptyState";
import { SettingSelect } from "../ui/SettingsControls";
import { entityRoute, SourceGrid } from "./SourceContent";
import { feedGap, FeedRow, type FeedItem } from "./HomeFeed";
import { usePageRefresh } from "../shell/pageRefresh";
import { usePageSource } from "../shell/pageSource";
import { HomeEntries, type HomeEntry } from "./HomeEntries";
import { HomeTracks } from "./HomeTracks";
import { NavigationScrollArea, NavigationScrollHeader } from "../ui/Chrome";
type HomeShelf={id:string;title:string;kind:string;items:any[];entries:HomeEntry[];more?:any;layout?:"feed"};
type HomeFeed={shelves:HomeShelf[];cached:boolean;fallbackCover?:string;next?:string|null};
type Categories={groups:{title:string;items:{id:string;title:string}[]}[];sorts?:{id:string;title:string}[]};
function sources(snapshot:PluginSnapshot, capability:string) {
  return snapshot.sources.filter(s=>s.enabled).flatMap(s=>{
    const p=snapshot.packages.find(p=>p.id===s.pluginId&&p.enabled&&p.capabilities.includes(capability));
    return p?[{id:s.sourceId,title:s.displayName||p.name,logo:p.logo,caps:p.capabilities}]:[];
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
function useToolbarSource(active:()=>boolean,state:ReturnType<typeof useSource>,onChange?:()=>void) {
  usePageSource({
    active, items:state.available, value:state.selected,
    select:(id)=>{onChange?.();void state.select(id).catch(console.error);},
  });
}
function MissingSource(props:{settings:()=>void}) {return <EmptyState title="还没有可用来源" description="在设置中启用支持此功能的音乐来源。" action={{label:"管理来源…",onPress:props.settings}} />;}
export function HomePage(props:Props) {
  const state=useSource(props,"home","homeSourceId");
  useToolbarSource(()=>props.active,state);
  const feeds=signal<Record<string,HomeFeed>>({}),errors=signal<Record<string,string>>({}),revision=signal(0);
  const loadingMore=signal(false),moreError=signal(""),wantMore=signal(false);
  const feedWidth=signal(0);
  const columns=()=>Math.max(1,Math.floor((feedWidth()+feedGap)/(token("listening-cover-size")+feedGap)));
  // Most rows are feed rows: a square cover as wide as its column, a title line and the row's bottom padding.
  const rowEstimate=()=>feedWidth()>0?Math.round((feedWidth()-feedGap*(columns()-1))/columns())+42:200;
  let ahead:{source:string;cursor:string;version:number;page:Promise<HomeFeed>}|null=null;
  let version=0,lastRefresh=0,closed=false;cleanup(()=>{closed=true;version++;});
  const current=()=>feeds()[state.selected()];
  // Load once the page is first shown; afterwards only a manual refresh (or a new source) reloads,
  // so entering and leaving the page never refetches the feed.
  const visited=signal(false);
  effect(()=>{if(props.active&&!visited())visited.set(true);});
  const identity=()=>visited()&&state.selected()?JSON.stringify([state.selected(),revision()]):undefined;
  const home=resource(identity,async key=>{
    const [source,refresh]=JSON.parse(key);
    const force=refresh>lastRefresh;lastRefresh=refresh;
    // Switching back to a source already loaded keeps its feed.
    if(!force&&feeds()[source]&&!feeds()[source].cached){moreError.set("");return;}
    const v=++version;errors.set({...errors(),[source]:""});
    ahead=null;loadingMore.set(false);moreError.set("");
    const valid=()=>!closed&&v===version&&identity()===key;
    try {
      const first=await props.core.sources.home(source,force) as HomeFeed;
      if(!valid())return;feeds.set({...feeds(),[source]:first});
      // A refreshed feed with as many rows as before does not raise onLoadMore again.
      if(force)wantMore.set(true);
      if(first.cached) {
        const fresh=await props.core.sources.home(source,true) as HomeFeed;
        if(valid())feeds.set({...feeds(),[source]:fresh});
      }
    }catch(e){if(valid())errors.set({...errors(),[source]:String(e)});}
  });
  const refresh=()=>revision.set(revision()+1);
  // Keep each virtual row bounded. A single unbounded waterfall would defeat recycling.
  type Section=HomeShelf&{continued?:boolean;feed?:FeedItem[]};
  // List matches rows by identity: feed items and unchanged rows stay the same objects
  // across appends, so a new page binds only its own rows instead of every mounted cell.
  const feedItems=new WeakMap<HomeShelf,FeedItem[]>();
  function itemsOf(s:HomeShelf){
    let items=feedItems.get(s);
    if(!items){
      items=(s.kind==="entries"?s.entries:s.items).map((item,index)=>({key:`${s.id}:${index}`,kind:s.kind,item}));
      feedItems.set(s,items);
    }
    return items;
  }
  function sectionRows(shelves:HomeShelf[]){
    const out:Section[]=[],perRow=columns();
    for(const s of shelves){
      const last=out[out.length-1];
      if(s.layout==="feed"){
        for (const item of itemsOf(s)) {
          const previous=out[out.length-1];
          if(previous?.feed&&previous.title===s.title&&previous.feed.length<perRow)previous.feed.push(item);
          else out.push({...s,id:item.key,continued:previous?.title===s.title,feed:[item]});
        }
      } else out.push({...s,continued:!last?.feed&&last?.title===s.title});
    }
    return out;
  }
  const sameRow=(a:Section,b:Section)=>a.title===b.title&&a.continued===b.continued&&a.kind===b.kind
    &&a.items===b.items&&a.entries===b.entries&&a.more===b.more
    &&(a.feed===b.feed||!!a.feed&&!!b.feed&&a.feed.length===b.feed.length&&a.feed.every((f,i)=>f===b.feed![i]));
  let rows=new Map<string,Section>();
  const merged=computed(()=>{
    const next=new Map<string,Section>();
    const out=sectionRows(current()?.shelves??[]).map(row=>{
      const old=rows.get(row.id),kept=old&&sameRow(old,row)?old:row;
      next.set(row.id,kept);return kept;
    });
    rows=next;return out;
  });
  // The shelves above the first feed row (song pages, recommendation covers, card rows) are
  // heavy, and a recycled cell rebuilds them whole when a fast scroll returns to the top, drawing
  // blank rows meanwhile. They live in the list's header, built once; feed rows stay recycled.
  const lead=computed(()=>{const all=merged(),first=all.findIndex(row=>!!row.feed);return first>0?all.slice(0,first):[];});
  const rest=computed(()=>{const skip=lead().length;return skip?merged().slice(skip):merged();});
  const openFeed=(e:FeedItem)=>e.kind==="entries"?link(e.item.to):props.navigate(entityRoute(e.item));
  // Append the next page when the scroll nears the bottom; shelves with repeated ids are skipped.
  // QQ continuation pages are small (one shelf) and take 0.5–4 s each, so one page is always
  // requested ahead: scrolling appends the buffered page at once and starts the next request.
  function prefetch(source:string,cursor:string){
    if(ahead?.source!==source||ahead.cursor!==cursor||ahead.version!==version){
      const page=props.core.sources.homePage(source,cursor) as Promise<HomeFeed>;
      ahead={source,cursor,version,page};
      page.catch(()=>{if(ahead?.page===page)ahead=null;});
    }
    return ahead.page;
  }
  effect(()=>{const feed=current();if(props.active&&feed?.next&&!feed.cached&&!home.loading()&&!moreError())void prefetch(state.selected(),feed.next).catch(()=>{});});
  async function more(){
    const source=state.selected(),feed=current(),v=version;
    if(!props.active||!feed?.next||feed.cached||loadingMore()||home.loading())return;
    loadingMore.set(true);moreError.set("");
    let cursor:string|null=feed.next,combined=feed.shelves;
    try{
      const rowsBefore=merged().length,seen=new Set(feed.shelves.map(s=>s.id)),cursors=new Set<string>();
      // Native load-more rearms when row count grows. Fill a partial row across
      // network pages and skip unsupported-only pages without waiting for another scroll.
      for(let batch=0;cursor&&batch<8;batch++){
        cursors.add(cursor);
        const page=await prefetch(source,cursor);
        if(closed||v!==version||state.selected()!==source||current()!==feed)return;
        if(page.next&&cursors.has(page.next))throw new Error("来源返回了重复的分页位置");
        const added=page.shelves.filter(s=>{if(seen.has(s.id))return false;seen.add(s.id);return true;});
        combined=[...combined,...added];cursor=page.next??null;
        if(sectionRows(combined).length>rowsBefore)break;
      }
      feeds.set({...feeds(),[source]:{...feed,shelves:combined,next:cursor}});
    }catch(e){if(v===version&&state.selected()===source){
      moreError.set(String(e));
      if(current()===feed&&combined!==feed.shelves)feeds.set({...feeds(),[source]:{...feed,shelves:combined,next:cursor}});
    }}
    finally{if(v===version)loadingMore.set(false);}
  }
  // The list raises onLoadMore once per row count. One that lands while the first page or
  // another page is still loading is kept, and runs as soon as the feed can continue.
  effect(()=>{
    const feed=current();
    if(wantMore()&&props.active&&feed?.next&&!feed.cached&&!home.loading()&&!loadingMore()&&!moreError()){
      wantMore.set(false);untracked(()=>void more());
    }
  });
  usePageRefresh(() => props.active, { run: refresh, disabled: () => !state.selected() || home.loading() });
  function link(link:any) {
    const ref=link.reference;
    if(ref.kind==="category"){props.navigate({kind:"categories",source:ref.source,category:ref.id});return;}
    if(ref.kind==="radio") {props.navigate({kind:"radio",source:ref.source});return;}
    props.navigate(entityRoute(link));
  }
  function section(s:()=>Section){
    return <View class="pb-6"><Shelf id={`shelf-${s().id}`} title={s().continued?"":s().title} tight={s().continued} more={s().more?()=>link(s().more):undefined}>
      <If when={!!s().feed}><FeedRow core={props.core} items={s().feed!} columns={columns()} open={openFeed} /></If>
      <If when={!s().feed&&s().kind==="tracks"}><HomeTracks core={props.core} tracks={s().items} active={props.active} width={feedWidth()} /></If>
      <If when={!s().feed&&s().kind==="entries"}><HomeEntries core={props.core} entries={s().entries} active={props.active}
        fallbackCover={current()?.fallbackCover} open={e=>link(e.to)} /></If>
      <If when={!s().feed&&s().kind!=="tracks"&&s().kind!=="entries"}><CardShelf items={s().items}
        listening hideTitle={s().kind==="toplists"} open={e=>props.navigate(entityRoute(e))} /></If>
    </Shelf></View>;
  }
  return <View testID="page-home" class="flex-1 min-h-0" style={{display:props.active?"flex":"none",flexDirection:"column"}}>
    <NavigationScrollArea>
    <List testID="home-scroll" class="flex-1" each={rest()} keyExtractor={s=>s.id}
      onLayout={e=>feedWidth.set(Math.max(0,e.bounds.size.width-token("content-inset-left")-token("content-inset-right")))}
      estimatedItemSize={rowEstimate()} loadMoreThreshold={2} onLoadMore={()=>wantMore.set(true)}
      contentClass="pl-(--content-inset-left) pr-(--content-inset-right) pb-(--dock-scroll-clearance)"
      header={<NavigationScrollHeader>
        <If when={!state.available().length}><MissingSource settings={props.settings} /></If>
        <If when={home.loading()&&!current()}><Skeleton label="正在加载推荐…" /></If>
        <If when={!!errors()[state.selected()]}><ErrorState cached={!!current()} message={errors()[state.selected()]} retry={refresh} /></If>
        <If when={!!current()&&!current().shelves.length}><Text class="text-sm text-muted-foreground">此来源暂无推荐内容</Text></If>
        <Each of={lead()} key={s=>s.id}>{section}</Each>
      </NavigationScrollHeader>}
      footer={<View>
        <If when={loadingMore()}><Spinner label="正在加载更多推荐" /></If>
        <If when={!!moreError()}><ErrorState message={moreError()} retry={()=>void more()} /></If>
        <If when={!!current()?.next&&!loadingMore()&&!moreError()}>
          <Button label="加载更多推荐" size="sm" variant="ghost" onPress={()=>void more()} />
        </If>
      </View>}>
        {section}
    </List>
    </NavigationScrollArea>
  </View>;
}
export function CategoriesPage(props:Props) {
  const state=useSource(props,"categories.list","categorySourceId"), category=signal(""),title=signal(""),sort=signal("");
  useToolbarSource(()=>props.active,state,()=>{category.set("");sort.set("");});
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
  usePageRefresh(() => props.active, {
    run: () => { groups.refetch(); if (category()) result.refetch(); },
    disabled: () => !state.selected() || groups.loading() || result.loading() || busy(),
  });
  return <View testID="page-categories" class="flex-1 min-h-0" style={{display:props.active?"flex":"none",flexDirection:"column"}}>
    <View class="pl-(--content-inset-left) pr-(--content-inset-right) pb-3 flex-row items-center justify-between gap-3"><Text role="header" class="text-xl font-semibold text-foreground">分类</Text></View>
    <If when={!state.available().length}><MissingSource settings={props.settings} /></If>
    <Scroll testID="categories-scroll" class="flex-1" contentClass="flex-col pl-(--content-inset-left) pr-(--content-inset-right) gap-4 pb-(--dock-scroll-clearance)">
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
  useToolbarSource(()=>props.active,state);
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
  usePageRefresh(() => props.active, {
    run: () => channels.refetch(),
    disabled: () => channels.loading() || !state.available().some(p => p.id === state.selected() && p.caps.includes("radio.channels")),
  });
  return <View testID="page-radio" class="flex-1 min-h-0" style={{display:props.active?"flex":"none",flexDirection:"column"}}>
    <View class="pl-(--content-inset-left) pr-(--content-inset-right) pb-3 flex-row items-center justify-between gap-3"><Text role="header" class="text-xl font-semibold text-foreground">电台</Text></View>
    <If when={!state.available().length}><MissingSource settings={props.settings} /></If>
    <If when={!!state.available().length}><View class="pl-(--content-inset-left) pr-(--content-inset-right) gap-3"><Shelf id="private-fm" title="为你播放"><View class="flex-row"><Button label="私人 FM" selected={!!radio()&&!radio().channel}
      variant={!!radio()&&!radio().channel?"primary":"secondary"} onPress={()=>void start()} /></View></Shelf>
      <If when={!!error()||!!channels.error()}><ErrorState message={error()||String(channels.error())} retry={()=>channels.refetch()} /></If>
      <If when={!!radio()?.error}><ErrorState message={radio().error} retry={()=>void start(radio().channel??undefined)} /></If>
      <If when={!!radio()}><Text class="text-sm text-muted-foreground">{radio().loading?"正在获取下一批曲目…":`持续播放 · 队列 ${player().tracks.length} 首`}</Text></If>
      <Text role="header" class="text-lg font-semibold text-foreground">频道</Text>
      <If when={channels.loading()}><Skeleton label="正在加载频道…" /></If>
    </View><List contentStyle={{paddingBottom:token("dock-scroll-clearance")}} testID="radio-channels" each={channels()??[]} keyExtractor={c=>c.id} estimatedItemSize={44} class="flex-1">
      {c=><View class="pl-(--content-inset-left) pr-(--content-inset-right) py-1 items-start"><Button label={c().title} selected={radio()?.channel===c().id} variant={radio()?.channel===c().id?"primary":"secondary"} onPress={()=>void start(c().id)} /></View>}
    </List></If>
  </View>;
}
