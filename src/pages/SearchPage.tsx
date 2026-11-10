import { cleanup, effect, List, resource, signal, Text, View } from "nsh";
import type { Core } from "haiku/native";
import type { PluginSnapshot } from "../state/plugins";
import { searchableSources, type SearchKind, type SearchRequest, type SearchPageResult } from "../state/search";
import type { Route } from "../shell/navigation";
import { SearchField } from "../ui/SearchField";
import { Button } from "../ui/Button";
import { SettingSelect } from "../ui/SettingsControls";
import { Dialog } from "../ui/Surfaces";
import { SourceGrid, SourceTracks } from "./SourceContent";

/** One retained search workspace; every response is checked against the full current query identity. */
export function SearchPage(props: {core:Core;active:boolean;snapshot:PluginSnapshot;request?:SearchRequest;navigate:(route:Route)=>void;settings:()=>void}) {
  const query=signal(props.request?.query ?? ""), kind=signal<SearchKind>(props.request?.kind ?? "track");
  const source=signal(props.request?.source ?? (props.core.settings.get() as any).searchSourceId ?? "");
  const note=signal("");
  const results=signal<any[]>([]), next=signal<string|null>(null), error=signal(""), moreBusy=signal(false), confirm=signal(false);
  const histories=signal<string[]>((props.core.history.list() as any).searches ?? []);
  props.core.history.onChange(() => histories.set((props.core.history.list() as any).searches ?? []));
  const available=()=>searchableSources(props.snapshot);
  const types=()=>available().find(s=>s.sourceId===source())?.kinds ?? [];
  let epoch=0,closed=false;
  cleanup(()=>{closed=true;epoch++;});
  effect(()=>{
    const sources=available();
    if (!sources.some(s=>s.sourceId===source())) source.set(sources[0]?.sourceId ?? "");
    const supported=types();
    if (supported.length && !supported.some(t=>t.id===kind())) kind.set(supported[0].id);
  });
  effect(()=>{
    const request=props.request;
    if(request) {query.set(request.query);if(request.source)source.set(request.source);if(request.kind)kind.set(request.kind);}
  });
  const identity=()=>props.active && source() && query().trim() ? JSON.stringify([source(),kind(),query().trim()]) : undefined;
  const first=resource(identity,async key=>{
    const version=++epoch;results.set([]);next.set(null);error.set("");moreBusy.set(false);
    const [s,k,q]=JSON.parse(key);
    const page=await props.core.sources.search(s,k,q,null) as SearchPageResult;
    if(closed || version!==epoch || identity()!==key)return;
    results.set(page.items);next.set(page.next ?? null);
  });
  function changeQuery(value:string) {epoch++;query.set(value);results.set([]);next.set(null);error.set("");moreBusy.set(false);}
  function submit(value=query()) { const q=value.trim();changeQuery(q);if(q)props.core.history.search(q);first.refetch(); }
  async function selectSource(value:string) {
    epoch++;source.set(value);error.set("");
    try {await props.core.settings.update({searchSourceId:value});}catch(e){error.set(String(e));}
  }
  async function more() {
    const key=identity(),version=epoch,cursor=next();if(!key||!cursor||moreBusy())return;
    moreBusy.set(true);error.set("");
    try {
      const [s,k,q]=JSON.parse(key);const page=await props.core.sources.search(s,k,q,cursor) as SearchPageResult;
      if(closed||version!==epoch||identity()!==key)return;
      results.set([...results(),...page.items]);next.set(page.next??null);
    }catch(e){if(!closed&&version===epoch&&identity()===key)error.set(String(e));}
    finally{if(!closed&&version===epoch&&identity()===key)moreBusy.set(false);}
  }
  function openEntity(route:Route) {
    if(route.kind==="program" && !props.snapshot.packages.find(p=>p.id===available().find(s=>s.sourceId===source())?.pluginId)?.capabilities.includes("program.get")) {note.set("此来源暂不提供节目详情，请选择其他结果。");return;}
    note.set("");props.navigate(route);
  }
  return <View testID="page-search" class="flex-1 min-h-0 pb-28" style={{display:props.active?"flex":"none",flexDirection:"column"}}>
    <View class="px-(--content-inset) gap-3 pb-3">
      <Text role="header" class="text-xl font-semibold text-foreground">搜索</Text>
      <View class="flex-row gap-2 items-center">
        <View class="flex-1"><SearchField label="搜索词" value={query()} onChange={changeQuery} onSubmit={()=>submit()} placeholder="搜索音乐、歌手或节目" /></View>
        <Button label="搜索" size="sm" onPress={()=>submit()} disabled={!query().trim()||!source()} />
        <SettingSelect label="来源" value={source()} items={available().map(s=>({value:s.sourceId,title:s.title}))} onChange={v=>void selectSource(v)} />
      </View>
      <View class="flex-row gap-1" testID="search-tabs"><Each of={types()} key={t=>t.id}>{t=><Button label={t().label} size="sm"
        variant={kind()===t().id?"primary":"ghost"} onPress={()=>{epoch++;kind.set(t().id);}} />}</Each></View>
    </View>
    <If when={!!note()}><Text class="px-(--content-inset) text-sm text-muted-foreground">{note()}</Text></If>
    <If when={!available().length}><View class="px-(--content-inset) gap-3"><Text class="text-sm text-muted-foreground">还没有可用的搜索来源</Text><Button label="管理来源…" onPress={props.settings} /></View></If>
    <If when={!query().trim()}>
      <View class="px-(--content-inset) flex-row gap-3 items-center pb-2"><Text class="flex-1 text-sm font-semibold text-foreground">搜索历史</Text>
        <If when={histories().length>0}><Button label="清空搜索历史…" size="sm" variant="ghost" onPress={()=>confirm.set(true)} /></If></View>
      <List testID="search-history" each={histories()} keyExtractor={q=>q} class="flex-1" estimatedItemSize={44}>
        {q=><View class="px-(--content-inset) py-1 flex-row gap-3 items-center"><View class="flex-1 items-start"><Button label={q()} variant="ghost" onPress={()=>submit(q())} /></View>
          <Button label="删除" accessibilityLabel={`删除搜索 ${q()}`} size="sm" variant="ghost" onPress={()=>props.core.history.remove("searches",q())} /></View>}
      </List>
      <If when={!histories().length}><Text class="px-(--content-inset) text-sm text-muted-foreground">还没有搜索记录</Text></If>
    </If>
    <If when={!!query().trim() && !!source()}>
      <If when={first.loading()}><Text class="px-(--content-inset) text-sm text-muted-foreground">正在搜索…</Text></If>
      <If when={!!first.error()||!!error()}><View class="px-(--content-inset) gap-2"><Text class="text-sm text-destructive">{error()||String(first.error())}</Text>
        <View class="flex-row gap-2"><Button label="重试搜索" size="sm" onPress={()=>{error.set("");first.refetch();}} /><Button label="去设置…" size="sm" onPress={props.settings} /></View></View></If>
      <If when={!first.loading()&&!first.error()&&!error()&&!results().length}><Text class="px-(--content-inset) py-4 text-sm text-muted-foreground">没有找到相关结果，试试其他关键词</Text></If>
      <If when={results().length>0}>
        <If when={kind()==="track"}><SourceTracks core={props.core} tracks={results()} active={props.active} fit /></If>
        <If when={kind()!=="track"}><View class="px-(--content-inset)"><SourceGrid items={results()} navigate={openEntity} /></View></If>
        <View class="px-(--content-inset) py-2 flex-row gap-3 items-center"><Text class="text-xs text-muted-foreground">{`已显示 ${results().length} 项`}</Text>
          <If when={!!next()}><Button label="加载更多结果" size="sm" loading={moreBusy()} onPress={()=>void more()} /></If></View>
      </If>
    </If>
    <Dialog open={confirm()} onOpenChange={e=>confirm.set(e.open)} class="w-96 max-w-full self-center mt-24 p-5 gap-4">
      <Text class="text-lg font-semibold text-foreground">清空搜索历史？</Text><Text class="text-sm text-muted-foreground">删除所有搜索记录。</Text>
      <View class="flex-row justify-end gap-2"><Button label="取消" onPress={()=>confirm.set(false)} /><Button label="清空" variant="destructive" onPress={()=>{props.core.history.clear("searches");confirm.set(false);}} /></View>
    </Dialog>
  </View>;
}
