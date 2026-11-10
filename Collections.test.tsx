import { expect, open, test } from "nsh/test";

test("native Dock likes, playlist submenu create/add, edit/delete, Stop and Core reopen", async () => {
  const app = await open("./CollectionsFixture.tsx", { platform: "macos", width: 1200, height: 800 });
  try {
  await app.waitFor("id:page-favorites");
  await app.waitFor("id:player-dock-1 >> role:button[喜欢]");
  await app.press("id:player-dock-1 >> role:button[喜欢]");
  await app.waitFor("id:player-dock-1 >> role:button[取消喜欢]");
  await app.waitFor("label:Song Alpha");
  await app.rightClick("label:Song Alpha");
  await app.press("text:加入歌单 ▸");
  await app.waitFor("role:menuitem[新建歌单…]");
  await app.press("role:menuitem[新建歌单…]");
  await app.waitFor("label:歌单名称");
  await app.type("label:歌单名称", "测试歌单");
  await app.press("保存歌单");
  await app.waitFor("text:测试歌单");
  await app.press("text:测试歌单");
  await app.waitFor("id:page-playlist");
  await app.waitFor("label:Song Alpha");
  await app.press("编辑歌单…");
  await app.clear("label:歌单名称");
  await app.type("label:歌单名称", "改名后的歌单");
  await app.type("label:歌单描述", "来自右键菜单");
  await app.press("保存歌单");
  await app.waitFor("text:改名后的歌单");
  await app.waitFor("text:来自右键菜单");
  await app.waitUntil(() => app.texts().some((t) => /^first-artwork:[1-9]/.test(t)));
  const originalArtwork = app.texts().find((t) => t.startsWith("first-artwork:"));
  await app.press("修改本地元数据");
  await app.waitFor("label:Song Alpha Edited");
  await app.waitUntil(() => app.texts().some((t) => t.startsWith("first-artwork:") && t !== originalArtwork));
  await app.press("删除歌单…");
  await app.waitFor("text:删除歌单？");
  await app.press("取消");
  expect(app.shows("text:改名后的歌单")).toBe(true);
  await app.press("删除歌单…");
  await app.press("确认删除歌单");
  await app.waitFor("id:page-playlists");
  await app.waitUntil(() => !app.shows("text:改名后的歌单"));
  await app.press("停止集合订阅");
  await app.waitUntil(() => app.texts().some((t) => t.includes("stopped:true")));
  const count = app.texts().find((t) => t.startsWith("events:"));
  await app.press("id:player-dock-1 >> role:button[取消喜欢]");
  await app.waitFor("id:player-dock-1 >> role:button[喜欢]");
  await app.press("id:player-dock-1 >> role:button[喜欢]");
  await app.waitFor("id:player-dock-1 >> role:button[取消喜欢]");
  expect(app.texts().find((t) => t.startsWith("events:"))).toBe(count);
  await app.press("验证重开");
  await app.waitFor("text:collections-reopened");
  } finally {
    if (app.shows("清理集合夹具")) {
      await app.press("清理集合夹具");
      await app.waitFor("text:collections-cleaned");
    }
  }
  console.log("collections: Dock → likes → submenu create/add → rename/delete confirmation → metadata/artwork refresh → Stop → Core reopen passed");
});
