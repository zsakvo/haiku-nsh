import { mount, resource, signal, Text, View } from "nsh";
import { Core } from "haiku/native";
import { HomePage } from "./src/pages/DiscoveryPages";
import { Button } from "./src/ui/Button";
import { createAppearance } from "./src/ui/theme";
import type { PluginSnapshot } from "./src/state/plugins";
import { createPageRefresh, PageRefreshContext } from "./src/shell/pageRefresh";

function Fixture() {
  const calls=signal(""),destination=signal(""),played=signal(""),fail=signal(false);
  const slow=signal(false),refresh=createPageRefresh();
  const ready=resource(async()=>{
    const core=await Core.openIsolated();
    await core.player.setVolume(0);
    const {sourceId}=await core.sources.startNetworkFixture() as {sourceId:string};
    await core.settings.update({homeSourceId:sourceId});
    const first=await core.sources.home(sourceId,false) as any;
    const song=first.shelves.find((s:any)=>s.kind==="tracks").items[0];
    const playlist=first.shelves.find((s:any)=>s.kind==="playlists").items[0];
    let request=0,generation=0;
    const page=(n:number,epoch:number)=>({
      cached:false,next:n<12?String(n+1):null,
      shelves:n===3?[]:[
        {id:`p${n}:playlist`,title:"为你推荐",kind:"playlists",layout:"feed",entries:[],
          items:[{...playlist,id:`list${n}`,title:`歌单 ${n}${epoch>1?` · 刷新${epoch}`:""}`,cover:undefined}]},
        {id:`p${n}:track`,title:"为你推荐",kind:"tracks",layout:"feed",entries:[],
          items:[{...song,title:`歌曲 ${n}`,cover:undefined}]},
        {id:`p${n}:album`,title:"为你推荐",kind:"albums",layout:"feed",entries:[],
          items:[{...playlist,id:`album${n}`,title:`专辑 ${n}`,cover:undefined,
            reference:{source:sourceId,kind:"album",id:`album${n}`}}]},
      ],
    });
    const sources={
      home:async()=>{calls.set(`请求 ${++request} · 首页`);return page(1,++generation);},
      homePage:async(_source:string,cursor:string)=>{
        const epoch=generation;
        calls.set(`请求 ${++request} · 第 ${cursor} 页`);
        await new Promise(resolve=>setTimeout(resolve,slow()?800:40));
        if(fail()){fail.set(false);throw new Error("续页暂时离线");}
        return page(Number(cursor),epoch);
      },
    };
    return {
      core:{sources,settings:core.settings,collections:core.collections,
        player:{play:async(tracks:any[],index:number)=>played.set(`播放 ${tracks[index].title}`)}} as unknown as Core,
      snapshot:await core.plugins.list() as PluginSnapshot,
    };
  });
  const appearance=createAppearance("light");
  return <PageRefreshContext.Provider value={refresh}><View class="flex-1 bg-background">
    <If when={!!ready()}><HomePage core={ready()!.core} active snapshot={ready()!.snapshot}
      route={{kind:"home"}} navigate={route=>destination.set(JSON.stringify(route))} settings={()=>{}} /></If>
    <View class="flex-row items-center gap-3 p-2">
      <Text testID="feed-calls">{calls()}</Text><Text>{destination()}</Text><Text>{played()}</Text>
      <Button label="下次续页失败" onPress={()=>fail.set(true)} />
      <Button label="深色" onPress={()=>void appearance.setMode("dark")} />
      <Button label="延迟续页" onPress={()=>slow.set(true)} />
      <Button label="刷新推荐夹具" onPress={()=>refresh.current()?.run()} />
    </View>
    <If when={!!ready.error()}><Text>{String(ready.error())}</Text></If>
  </View></PageRefreshContext.Provider>;
}
mount(Fixture);
