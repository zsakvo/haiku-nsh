import { cleanup, effect, Scroll, signal, View } from "nsh";
import { PluginService, type Core } from "haiku/native";
import { Button } from "../ui/Button";
import { ErrorState, RecommendationCard, type RecommendationPreview } from "../ui/Discovery";

export type HomeEntry = {
  title:string;cover?:string;
  to:{reference:{source:string;kind:string;id:string}};
};
const playable = (entry:HomeEntry) => ["playlist","radio","album","toplist","program"].includes(entry.to.reference.kind);

/** Entry playback stays on Home; navigation still opens the source's original destination. */
export function HomeEntries(props:{
  core:Core;entries:HomeEntry[];active:boolean;fallbackCover?:string;open:(entry:HomeEntry)=>void;
}) {
  const pending=signal(""),error=signal(""),failed=signal<HomeEntry|null>(null);
  const previews=signal<Record<string,RecommendationPreview>>({});
  let generation=0,previewGeneration=0,closed=false;
  cleanup(()=>{closed=true;generation++;});
  effect(()=>{
    props.entries;props.active;
    generation++;previewGeneration++;pending.set("");error.set("");failed.set(null);previews.set({});
  });
  const cards=()=>props.entries.filter(playable);
  const shortcuts=()=>props.entries.filter(e=>!playable(e));
  async function preview(entry:HomeEntry) {
    const ref=entry.to.reference,key=JSON.stringify(ref);
    if(previews()[key]&&!previews()[key].error)return;
    const epoch=previewGeneration;
    const valid=()=>!closed&&epoch===previewGeneration&&props.active;
    previews.set({...previews(),[key]:{titles:[],loading:true}});
    try {
      let titles:string[]=[];
      if(ref.kind==="radio") {
        const state=props.core.player.snapshot() as any;
        if(state.radio?.source===ref.source&&state.radio?.channel===ref.id) {
          titles=(state.tracks??[]).slice(state.index??0).slice(0,5).map((t:any)=>t.title);
        }
        if(!titles.length) {
          const response=await props.core.sources.invoke(ref.source,"radio.next",[ref.id],{history:[],count:5}) as any;
          titles=(response.value??[]).slice(0,5).map((t:any)=>t.title);
        }
      } else {
        const reference=PluginService.reference(ref.source,ref.kind,ref.id),seen=new Set<string>();
        let cursor:string|null=null;
        do {
          const page=await props.core.sources.detail(reference,cursor) as any;
          if(!valid())return;
          titles.push(...(page.tracks?.items??[]).map((t:any)=>t.title));
          cursor=page.tracks?.next??null;
          if(cursor&&seen.has(cursor))break;
          if(cursor)seen.add(cursor);
        } while(titles.length<5&&cursor);
      }
      if(valid())previews.set({...previews(),[key]:{titles:titles.slice(0,5),loading:false}});
    } catch(e) {
      if(valid())previews.set({...previews(),[key]:{titles:[],loading:false,error:String(e)}});
    }
  }
  async function play(entry:HomeEntry) {
    if(pending())return;
    const version=++generation;
    const valid=()=>!closed&&version===generation&&props.active;
    const ref=entry.to.reference;
    pending.set(JSON.stringify(ref));error.set("");failed.set(null);
    try {
      if(ref.kind==="radio") {
        await props.core.player.startRadio(ref.source,ref.id);
      } else {
        const reference=PluginService.reference(ref.source,ref.kind,ref.id);
        const tracks:any[]=[],seen=new Set<string>();
        let cursor:string|null=null,info:any;
        do {
          const page=await props.core.sources.detail(reference,cursor) as any;
          if(!valid())return;
          info??=page.info;tracks.push(...(page.tracks?.items??[]));
          cursor=page.tracks?.next??null;
          if(cursor&&seen.has(cursor))throw new Error("来源返回了重复的分页位置");
          if(cursor)seen.add(cursor);
        } while(cursor);
        if(!tracks.length)throw new Error("此列表暂无可播放曲目");
        await props.core.player.setMode("sequential");
        if(!valid())return;
        await props.core.player.play(tracks,0);
        if(valid()&&ref.kind==="playlist")props.core.history.recordPlaylist({
          kind:"remote",sourceId:ref.source,id:ref.id,title:info?.title??entry.title,
          cover:info?.cover??entry.cover,trackCount:tracks.length,
        });
      }
    } catch(e) {
      if(valid()){error.set(String(e));failed.set(entry);}
    } finally {
      if(valid())pending.set("");
    }
  }
  return <View class="gap-2 shrink-0">
    <If when={!!cards().length}>
      <Scroll testID="recommendation-covers" horizontal class="h-(--recommendation-cover-height) shrink-0"
        contentClass="flex-row gap-(--recommendation-cover-gap)">
        <Each of={cards()} key={e=>JSON.stringify(e.to.reference)}>{e=>
          <RecommendationCard title={e().title} cover={e().cover||props.fallbackCover}
            preview={previews()[JSON.stringify(e().to.reference)]} loadPreview={()=>void preview(e())}
            busy={pending()===JSON.stringify(e().to.reference)} disabled={!!pending()}
            open={()=>props.open(e())} play={()=>void play(e())} />}
        </Each>
      </Scroll>
    </If>
    <If when={!!shortcuts().length}>
      <View class="flex-row flex-wrap gap-2"><Each of={shortcuts()} key={e=>JSON.stringify(e.to.reference)}>{e=>
        <Button label={e().title} size="sm" onPress={()=>props.open(e())} />}
      </Each></View>
    </If>
    <If when={!!error()}><ErrorState message={error()} retry={()=>{const entry=failed();if(entry)void play(entry);}} /></If>
  </View>;
}
