import { expect, open, test } from "nsh/test";

test("scrape review omits an empty year and resolves the installed provider name", async () => {
  const app = await open("../../../PolishReviewPreview.tsx", { platform: "macos", width: 1200, height: 800 });
  await app.waitFor("id:scrape-candidates >> role:button[匹配后的歌曲 · 80 分 · QQ 音乐刮削]");
  expect(app.shows("id:scrape-differences >> text:年份")).toBe(false);
  expect(app.shows("id:scrape-differences >> text:→ 潮汐与回声")).toBe(true);
});
