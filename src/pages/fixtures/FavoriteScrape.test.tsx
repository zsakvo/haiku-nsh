import { expect, open, test } from "nsh/test";

const options = { platform: "macos" as const, width: 1200, height: 800, network: "live" as const };

test("favorite opens complete refreshed mirror and unfavorite requires confirmation", async () => {
  const app = await open("../../../FavoriteRefreshPreview.tsx", options);
  await app.waitFor("label:潮汐之间");
  await app.waitFor("label:远方回声");
  await app.waitFor("text:2 首歌曲");
  await app.press("取消收藏歌单…");
  await app.waitFor("role:button[确认取消收藏]");
  await app.press("role:button[取消]");
  expect(app.shows("label:潮汐之间")).toBe(true);
  await app.press("取消收藏歌单…");
  await app.press("确认取消收藏");
  await app.waitUntil(() => !app.shows("role:button[取消收藏歌单…]"));
});

test("failed favorite refresh keeps offline tracks and reports the fallback", async () => {
  const app = await open("../../../FavoriteRefreshOfflinePreview.tsx", options);
  await app.waitFor("label:离线缓存曲目");
  await app.waitFor("text:刷新失败，显示离线内容");
  expect(app.shows("text:夜航离线快照")).toBe(true);
});

test("batch scrape reviews differences, accepts, skips and undoes automatic metadata", async () => {
  const app = await open("../../../ScrapePreview.tsx", options);
  await app.waitFor("label:Song Alpha");
  await app.press("label:Song Alpha");
  await app.keyboard.press("mod+a");
  await app.waitFor("text:已选择 3 首");
  await app.press("role:button[刮削…]");
  await app.press("开始刮削");
  await app.waitFor("text:刮削完成 · 3 / 3");
  await app.waitFor("id:scrape-differences >> text:Fixture Album");
  await app.waitFor("id:scrape-differences >> text:→ 刮削专辑");
  await app.press("采用候选");
  await app.waitFor("role:button[Song Beta · 已采用]");
  await app.press("role:button[Elsewhere · 无匹配]");
  await app.press("跳过");
  await app.waitFor("role:button[Elsewhere · 已跳过]");
  await app.press("role:button[Song Alpha · 已采用 · 自动]");
  await app.press("撤销采用");
  await app.waitFor("role:button[Song Alpha · 待审阅 · 自动]");
  await app.press("关闭审阅");
  await app.rightClick("label:Song Beta");
  await app.waitFor("role:button[刮削…]");
});
