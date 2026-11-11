import { expect, open, test } from "nsh/test";

test("lyrics exports preview/adopt, persist across Core reopen, isolate tracks and reset", async () => {
  const app = await open("./LyricsFixture.tsx", { platform: "macos", width: 1200, height: 800 });
  await app.waitFor("text:歌词就绪 Alpha");
  await app.press("采用夹具歌词");
  await app.waitFor("text:手动歌词");
  await app.press("重开歌词 Core");
  await app.waitFor("text:Core 已重开");
  await app.waitFor("text:歌词就绪 Alpha");
  await app.waitFor("text:手动歌词");
  await app.press("切换歌词曲目");
  await app.waitFor("text:歌词就绪 Beta");
  expect(app.shows("text:手动歌词")).toBe(false);
  await app.press("返回歌词曲目");
  await app.waitFor("text:歌词就绪 Alpha");
  await app.waitFor("text:手动歌词");
  await app.press("恢复自动歌词");
  await app.waitFor("text:暂无歌词");
  await app.press("空来源搜索");
  await app.waitFor("text:空来源候选 0");
});
