import { cleanup, effect, signal, Text, View } from "nsh";
import { Command, type CommandItem } from "nsh/base";
import { Dialog } from "../ui/Surfaces";
import { Kbd } from "../ui/Navigation";
import { Icon } from "../ui/Icon";
import { CommandKeyboard, type Core } from "haiku/native";
import { entityRoute } from "../pages/SourceContent";
import type { Route } from "./navigation";
import type { SearchPreview, SearchRequest } from "../state/search";

export function CommandPalette(props: { open: boolean; items: CommandItem[]; core?: Core; initialQuery?:string; navigate:(route:Route)=>void; search:(request:SearchRequest)=>void; onClose: () => void; onSelect: (id: string) => void }) {
  const query=signal(props.initialQuery??""), groups=signal<SearchPreview[]>([]), loading=signal(false), error=signal("");
  let epoch=0;
  cleanup(()=>{epoch++;});
  effect(()=>{if(props.open)query.set(props.initialQuery??"");});
  effect(()=>{
    const q=query().trim(),core=props.core,open=props.open,version=++epoch;
    groups.set([]);error.set("");loading.set(!!q&&open);
    if(!q||!open||!core){loading.set(false);return;}
    const timer=setTimeout(()=>{
      void core.sources.searchPreview(q).then(value=>{
        if(version===epoch&&props.open&&query().trim()===q)groups.set(value as SearchPreview[]);
      }).catch(e=>{if(version===epoch&&props.open)error.set(String(e));})
        .finally(()=>{if(version===epoch)loading.set(false);});
    },280);
    return ()=>clearTimeout(timer);
  });
  function items():CommandItem[] {
    const q=query().trim();
    const navigation=props.items.filter(i=>!q||i.label.toLowerCase().includes(q.toLowerCase())||(i.keywords??[]).some(k=>k.includes(q)));
    if(!q)return navigation;
    return [...navigation,...groups().flatMap(g=>[
      ...g.playlists.slice(0,3).map(p=>({id:JSON.stringify(["playlist",g.source,p.reference.id]),label:`歌单 · ${p.title}`,group:g.title})),
      ...g.tracks.slice(0,3).map(t=>{
        const artists=(t.artists??[]).map((a:string|{name:string})=>typeof a==="string"?a:a.name).filter(Boolean).join("、");
        return {id:JSON.stringify(["track",g.source,t.id]),label:artists?`${t.title} · ${artists}`:t.title,group:g.title};
      }),
      ...g.errors.map((message,i)=>({id:JSON.stringify(["error",g.source,i]),label:message,group:g.title,disabled:true})),
    ]),{id:"search-all",label:"在搜索页查看全部",group:"搜索"}];
  }
  async function choose(id:string) {
    if(id==="search-all"){props.search({query:query().trim()});props.onClose();return;}
    if(!id.startsWith("[")){props.onSelect(id);return;}
    const [kind,source,key]=JSON.parse(id),group=groups().find(g=>g.source===source);
    if(!group)return;
    if(kind==="playlist"){
      const playlist=group.playlists.find(p=>p.reference.id===key);if(playlist){props.navigate(entityRoute(playlist));props.onClose();}
    }else if(kind==="track"&&props.core){
      const index=group.tracks.findIndex(t=>t.id===key);if(index<0)return;
      try{props.core.history.search(query().trim());await props.core.player.play(group.tracks,index);props.onClose();}
      catch(e){error.set(String(e));}
    }
  }
  return <Dialog open={props.open} class="w-128 max-w-full self-center mt-24 overflow-hidden"
    onCancel={props.onClose}
    onOpenChange={(event) => { if (!event.open) props.onClose(); }}>
    <CommandKeyboard open={props.open} onCancel={props.onClose}>
    <Command items={items()} query={query()} onQueryChange={e=>query.set(e.query)} filterable={false} loading={loading()} accessibilityLabel="命令面板" searchAccessibilityLabel="查找命令"
      autoFocus placeholder="前往页面或搜索音乐…" onCancel={props.onClose}
      onSelect={(event) => void choose(event.id)}
      searchSuffix={<Text class="text-xs text-muted-foreground">{error() || (loading()?"正在搜索…":"")}</Text>}
      searchPrefix={<Icon name="search" small color="muted-foreground" />}
      searchBarClass="flex-row items-center gap-2 px-4 py-3 border-b border-border"
      searchClass="flex-1 text-base text-foreground placeholder:text-muted-foreground"
      listClass="max-h-96 p-1.5"
      itemClass="group min-h-8 rounded-(--radius) px-3 py-1 hover:bg-accent focus:bg-selection active:bg-muted text-sm text-popover-foreground"
      groupClass="px-3 pt-3 pb-1 text-xs text-muted-foreground"
      separatorClass="h-px my-2 bg-border"
      renderItem={(item) => <View class="flex-1 flex-row items-center justify-between gap-4">
        <Text class="text-sm text-popover-foreground">{item.label}</Text>
        {item.shortcutLabel && <Kbd label={item.shortcutLabel} />}
      </View>}
      empty={<Text class="p-6 text-sm text-muted-foreground">没有匹配的命令</Text>}
      footer={<View class="flex-row items-center gap-2 px-4 py-2 border-t border-border">
        <Kbd label="↑ ↓" /><Text class="text-xs text-muted-foreground">选择</Text>
        <Kbd label="↵" /><Text class="text-xs text-muted-foreground">前往</Text>
        <View class="flex-1" /><Kbd label="Esc" /><Text class="text-xs text-muted-foreground">关闭</Text>
      </View>} />
    </CommandKeyboard>
  </Dialog>;
}
