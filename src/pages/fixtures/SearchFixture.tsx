import { cleanup, resource, signal, Text, View } from "nsh";
import { Core } from "haiku/native";
import { AppShell } from "../../shell/AppShell";
import { Button } from "../../ui/Button";
export function SearchFixture(props:{page?:"tracks"|"albums"|"history"|"command"|"library";controls?:boolean}) {
  const queue=signal(""),calls=signal("");let stop:(()=>void)|undefined;
  cleanup(()=>stop?.());
  const ready=resource(async()=>{
    const core=await Core.openIsolated();const fixture=await core.sources.startSearchFixture() as {sourceId:string;secondSourceId:string};
    await core.player.setVolume(0);
    await core.settings.update({searchSourceId:fixture.sourceId});
    core.history.search("坂本龙一");core.history.search("夜航");core.history.search("潮汐");
    stop=core.player.onState(s=>{const state=s as any;queue.set(`队列 ${state.tracks.length} 首 · ${state.current?.title??"空"}`);});
    console.log("M4-02 isolated search fixture ready");return {core,fixture};
  });
  return <View class="flex-1"><If when={!!ready()}>
    <AppShell initialMode="light" initialRoute={props.page==="library"?{kind:"source",source:ready()!.fixture.sourceId}:{kind:"search"}}
      initialSearch={{query:props.page==="history"||props.controls?"":"潮汐",source:ready()!.fixture.sourceId,kind:props.page==="albums"?"album":"track"}}
      initialOverlay={props.page==="command"?"command":undefined} initialCommandQuery={props.page==="command"?"潮汐":undefined} />
    <If when={props.controls}><View class="flex-row gap-2"><Text>{queue()}</Text><Text>{calls()}</Text>
      <Button label="读取搜索调用" onPress={()=>void ready()!.core.sources.invoke(ready()!.fixture.sourceId,"track.get",["search-counts"],{}).then((v:any)=>calls.set(`搜索调用 ${v.value?.title??v.title}`))} />
    </View></If>
  </If><If when={!!ready.error()}><Text>{String(ready.error())}</Text></If></View>;
}
