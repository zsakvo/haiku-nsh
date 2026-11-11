import { mount,resource,Text,View } from "nsh";
import { Core } from "haiku/native";
import { AppShell } from "./src/shell/AppShell";
/** Real native acceptance, using an explicit artifact data root, not an isolated fixture. */
function DesktopLyricsRun() {
  const ready=resource(async()=>{
    const core=await Core.openIn("/Users/zsakvo/Project/GPUI/haiku-nsh/.artifacts/m6-03/data");
    const fixture=await core.sources.startNetworkFixture() as {sourceId:string;url:string};
    await core.settings.update({themeMode:"light",lyricsAutoSearch:true,
      lyricsProvidersDisabled:["betterlyrics","lrclib","kugou","paxsenix","lyricsplus"],
      lyricsCustomProviders:[{id:"desktop-proof",name:"离线逐字歌词",urlTemplate:`${fixture.url}/lyrics`,responseType:"json",jsonPath:"text",headers:{}}],
      desktopLyrics:{visible:true,locked:false,background:"solid",fontSize:36,layout:"classic",showTranslation:true}});
    const track={source:fixture.sourceId,id:"1",title:"潮汐之间",artists:["林间回声"],duration:60};
    core.collections.setLiked(track,true);await core.player.setVolume(0.08);await core.player.play([track],0);await core.player.setMode("repeatOne");return core;
  });
  return <View class="flex-1"><If when={!!ready()}><AppShell initialMode="light" /></If><If when={!!ready.error()}><Text>{String(ready.error())}</Text></If></View>;
}
mount(DesktopLyricsRun);
