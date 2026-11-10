import { expect, open, test } from "nsh/test";

test("local library scan, groups, recycled selection, sparse batch edit and filtered playback", async () => {
  const app = await open("./LibraryFixture.tsx", { platform: "macos", width: 1200, height: 800 });
  try {
    await app.waitFor("text:你的音乐，放在这里");
    await app.press("添加文件夹…");
    await app.waitFor("label:Song Alpha");
    expect(app.texts()).toContain("Artist A");
    expect(app.texts()).toContain("Artist B");
    console.log("library: empty → add isolated root → scan 123 playable tracks");

    await app.press("text:专辑");
    await app.press("Fixture Album · Various · 2 首");
    await app.waitFor("label:Song Beta");
    expect(app.shows("label:Elsewhere")).toBe(false);
    await app.keyboard.press("mod+[");
    await app.waitFor("text:歌曲");
    await app.press("text:歌曲");
    await app.press("label:Song Alpha");
    await app.scroll("id:page-library", 4800);
    await app.waitFor("label:Zzz 100");
    expect(app.shows("label:Song Alpha")).toBe(false);
    await app.scroll("id:page-library", -4800);
    await app.waitFor("label:Song Alpha");
    expect(app.texts()).toContain("已选择 1 首");
    console.log("library: album EntityRef/history and virtual cell recycling preserve ID selection");

    await app.keyboard.down("mod");
    await app.press("label:Song Beta");
    await app.keyboard.up("mod");
    await app.waitFor("text:已选择 2 首");
    await app.press("编辑信息…");
    await app.waitFor("text:批量编辑 2 首歌曲");
    await app.clear("label:专辑");
    await app.type("label:专辑", "Edited Album");
    await app.press("保存");
    await app.waitUntil(() => app.count("text:Edited Album") === 2);
    expect(app.shows("label:Song Alpha")).toBe(true);
    expect(app.shows("label:Song Beta")).toBe(true);
    expect(app.texts()).toContain("Artist A");
    expect(app.texts()).toContain("Artist B");
    console.log("library: sparse batch album edit updates both rows, preserving distinct titles/artists");

    await app.type("label:筛选本地曲库", "Song");
    await app.waitUntil(() => !app.shows("label:Elsewhere"));
    // Headless press preserves the platform's click-count semantics.
    await app.press("label:Song Beta");
    await app.press("label:Song Beta");
    await app.waitFor("text:status:playing;current:Song Beta;queue:Song Alpha|Song Beta");
    console.log("library: double press → core.player playing Beta, complete filtered queue [Alpha, Beta]");
    await app.rightClick("label:Song Beta");
    for (const label of ["播放", "下一首播放", "加入队列", "编辑信息…", "在 Finder 中显示"])
      expect(app.shows(`role:menuitem[${label}]`)).toBe(true);
    expect(app.count("role:menuitem")).toBe(5);
    console.log("library: context menu has exactly the five implemented commands");
    await app.keyboard.press("escape");
  } finally {
    if (app.shows("清理测试数据")) {
      await app.press("清理测试数据");
      await app.waitFor("text:fixture-cleaned");
    }
  }
});
