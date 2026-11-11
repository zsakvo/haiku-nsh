import { open, test } from "nsh/test";
test("menubar lyrics and custom highlight survive Core reopen",async()=>{
  const app=await open("../../../MenubarSettingsPreview.tsx",{platform:"macos",width:1200,height:950});
  await app.waitFor("label:菜单栏歌词");
  await app.press("label:菜单栏歌词");
  await app.press("label:菜单栏逐字高亮");
  await app.press("role:option[自定义颜色]");
  await app.waitFor("label:菜单栏高亮颜色");
  await app.clear("label:菜单栏高亮颜色");await app.type("label:菜单栏高亮颜色","#E06C75");
  await app.press("保存高亮颜色");
  await app.press("重开并读取菜单栏设置");
  await app.waitFor("text:菜单栏持久化 false #E06C75");
});
