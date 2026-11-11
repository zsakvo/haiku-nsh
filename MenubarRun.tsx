import { mount, resource, Text, View } from "nsh";
import { Core } from "haiku/native";
import { AppShell } from "./src/shell/AppShell";
/** Real AppKit acceptance entry, using an explicit artifact data directory.
 * Unlike isolated Headless fixtures, this starts the normal native integration. */
function MenubarRun() {
  const ready=resource(async()=>{
    const core=await Core.openIn("/Users/zsakvo/Project/GPUI/haiku-nsh/.artifacts/m6-02/data");
    const fixture=await core.sources.startNetworkFixture() as {sourceId:string;url:string};
    await core.settings.update({themeMode:"light",lyricsAutoSearch:true,
      lyricsProvidersDisabled:["betterlyrics","lrclib","kugou","paxsenix","lyricsplus"],
      lyricsCustomProviders:[{id:"menubar-proof",name:"离线逐字歌词",urlTemplate:`${fixture.url}/lyrics`,responseType:"json",jsonPath:"text",headers:{}}],
      menubarLyricsVisible:true,menubarLyricHighlight:"system"});
    const track={source:fixture.sourceId,id:"1",title:"潮汐之间",artists:["林间回声"],duration:60,cover:`${fixture.url}/cover.png`};
    core.collections.create("菜单栏测试歌单");core.collections.setLiked(track,true);
    await core.player.setVolume(0.08);
    await core.player.play([track],0);await core.player.setMode("repeatOne");
    return core;
  });
  return <View class="flex-1"><If when={!!ready()}><AppShell initialMode="light" /></If>
    <If when={!!ready.error()}><Text>{String(ready.error())}</Text></If></View>;
}
mount(MenubarRun);
