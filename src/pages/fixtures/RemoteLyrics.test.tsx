import { expect, open, test } from "nsh/test";
const options = { platform: "macos" as const, width: 1200, height: 800, network: "live" as const };

test("Escape closes now playing and the dock can reopen it", async () => {
  const app = await open("../../../RemoteLyricsPreview.tsx", options);
  await app.waitFor("label:显示歌词");
  await app.press("label:显示歌词");
  await app.waitFor("id:now-playing");
  await app.keyboard.press("escape");
  await app.waitUntil(() => !app.shows("id:now-playing"));
  await app.press("label:显示歌词");
  await app.waitFor("id:now-playing");
  await app.press("label:收起正在播放");
  await app.waitUntil(() => !app.shows("id:now-playing"));
});

test("manual lyrics dialog previews, adopts, restores automatic and reports source errors", async () => {
  const app = await open("../../../RemoteLyricsPreview.tsx", options);
  await app.waitFor("text:潮汐之间");
  await app.press("label:显示歌词");
  await app.waitFor("id:now-playing");
  await app.press("role:button[查找歌词…]");
  await app.waitFor("id:lyrics-preview >> text:潮汐之间");
  await app.waitUntil(() => app.texts().some(t => t.startsWith("离线来源：")));
  await app.waitUntil(() => app.texts().some(t => t.includes("分") && t.includes("逐字")));
  await app.press("采用歌词");
  await app.waitUntil(() => !app.shows("text:查找歌词"));
  await app.press("label:显示歌词");
  await app.waitFor("id:now-playing");
  await app.press("role:button[查找歌词…]");
  await app.waitFor("id:lyrics-preview >> text:潮汐之间");
  await app.press("恢复自动");
  await app.waitUntil(() => !app.shows("text:查找歌词"));
  await app.press("label:显示歌词");
  await app.waitFor("id:now-playing");
});

test("lyrics settings disable a source without requests and edit custom HTTP configuration", async () => {
  const app = await open("../../../LyricsSettingsFixture.tsx", options);
  await app.waitFor("id:lyrics-settings");
  await app.press("label:启用 回声歌词");
  await app.press("检查禁用请求数");
  await app.waitFor("text:禁用检查 0 项 0 次请求");
  await app.press("label:编辑 回声歌词");
  await app.clear("label:来源名称"); await app.type("label:来源名称", "回声改名");
  await app.press("保存来源");
  await app.waitFor("text:回声改名");
  expect(app.shows("label:启用 回声改名")).toBe(true);
  await app.press("label:删除 回声改名");
  await app.waitUntil(() => !app.shows("text:回声改名"));
  await app.press("添加 HTTP 来源");
  await app.type("label:来源名称", "新增歌词");
  await app.type("label:URL 模板", "http://127.0.0.1:9/lyrics?title={title}");
  await app.press("保存来源");
  await app.waitFor("label:启用 新增歌词");
});
