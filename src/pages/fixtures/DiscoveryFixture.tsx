import { cleanup, resource, signal, Text, View } from "nsh";
import { Core } from "haiku/native";
import { AppShell } from "../../shell/AppShell";
import { Button } from "../../ui/Button";
export function DiscoveryFixture(props:{page?:"home"|"tags"|"playlists"|"radio";scenario?:"offline"|"failure";controls?:boolean}) {
  const queue=signal(""),note=signal("");let stop:(()=>void)|undefined;cleanup(()=>stop?.());
  const ready=resource(async()=>{
    const core=await Core.openIsolated();const fixture=await core.sources.startNetworkFixture() as {sourceId:string};
    await core.player.setVolume(0);await core.settings.update({homeSourceId:fixture.sourceId,categorySourceId:fixture.sourceId,radioSourceId:fixture.sourceId});
    if(props.scenario==="offline"){
      await core.sources.home(fixture.sourceId,false);
      await core.sources.invoke(fixture.sourceId,"track.get",["home-offline"],{});
    }
    if(props.scenario==="failure")await core.sources.invoke(fixture.sourceId,"track.get",["home-fail"],{});
    if(props.page==="radio")await core.player.startRadio(fixture.sourceId,"forest");
    stop=core.player.onState(s=>{const p=s as any;queue.set(`队列 ${p.tracks.length} 首 · ${p.current?.title??"空"}`);});
    console.log(`M4-03 isolated discovery fixture ready: ${props.page??"home"} ${props.scenario??"online"}`);
    return {core,fixture};
  });
  return <View class="flex-1"><If when={!!ready()}>
    <AppShell initialMode="light" initialRoute={props.page==="tags"?{kind:"categories"}:props.page==="playlists"?{kind:"categories",source:ready()!.fixture.sourceId,category:"cn"}:props.page==="radio"?{kind:"radio"}:{kind:"home"}} />
    <If when={props.controls}><View class="flex-row gap-2 items-center"><Text>{queue()}</Text><Text>{note()}</Text>
      <Button label="下一首夹具" onPress={()=>void ready()!.core.player.next()} />
      <Button label="恢复来源夹具" onPress={()=>void ready()!.core.sources.invoke(ready()!.fixture.sourceId,"track.get",["home-online"],{}).then(()=>note.set("来源已恢复"))} />
    </View></If>
  </If><If when={!!ready.error()}><Text>{String(ready.error())}</Text></If></View>;
}
