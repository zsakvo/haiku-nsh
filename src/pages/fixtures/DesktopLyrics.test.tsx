import { open,test } from "nsh/test";
test("desktop lyrics preferences and app shortcut persist without an AppKit panel",async()=>{
  const app=await open("../../../DesktopLyricsSettingsPreview.tsx",{platform:"macos",width:1000,height:1000});
  await app.waitFor("id:desktop-lyrics-settings");
  await app.press("快捷键切换桌面歌词");
  await app.press("label:锁定桌面歌词");
  await app.press("label:桌面歌词布局");await app.press("role:option[舞台]");
  await app.press("label:桌面歌词字重");await app.press("role:option[半粗]");
  await app.press("label:桌面歌词配色：晴空");
  await app.press("label:桌面歌词阴影");await app.press("label:桌面歌词描边");await app.press("label:桌面歌词显示译文");
  await app.press("label:桌面歌词背景");await app.press("role:option[纯色]");
  await app.press("重开桌面歌词设置");
  await app.waitFor("text:桌面持久化 true true stage semibold solid #FFFFFF #4A9EFF false false false");
});
test("native settings requests select Lyrics again when the settings overlay is already open",async()=>{
  const app=await open("../../../DesktopLyricsNavigationPreview.tsx",{platform:"macos",width:1200,height:950});
  await app.waitFor("role:header[播放]");
  await app.press("请求歌词设置");await app.waitFor("id:desktop-lyrics-settings");
  await app.press("role:button[播放]");await app.waitFor("role:header[播放]");
  await app.press("请求歌词设置");await app.waitFor("id:desktop-lyrics-settings");
});
