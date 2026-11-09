import { expect, open, test } from "nsh/test";

test("S6 jumps to row 9000 and preserves selection by ID through recycling", async () => {
  const app = await open("./List.tsx", {
    platform: "macos",
    width: 1000,
    height: 700,
  });
  await app.press("role:button[第 9000 行]");
  await app.waitFor("text:曲目 09000");
  expect(app.shows("text:曲目 09000")).toBe(true);

  await app.press("role:button[回到顶部]");
  await app.waitFor("text:曲目 00005");
  await app.press("text:曲目 00005");
  await app.waitFor("text:曲目 00005 · 已选中");
  await app.press("role:button[滚动到底部]");
  await app.waitFor("text:曲目 10000");
  expect(app.texts().filter((text) => text.includes("已选中")).length).toBe(0);
  await app.press("role:button[回到顶部]");
  await app.waitFor("text:曲目 00005 · 已选中");
  expect(app.texts().filter((text) => text.includes("已选中"))).toEqual([
    "曲目 00005 · 已选中",
  ]);
});
