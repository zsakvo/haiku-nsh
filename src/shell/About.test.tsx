import { expect, open, test } from "nsh/test";

test("About opens the shipped Rust dependency notices and preserves the undeclared app license", async () => {
  const app = await open("../../AboutPreview.tsx", {
    platform: "macos", width: 1200, height: 900,
  });
  await app.waitFor("id:about-content");
  expect(app.texts()).toContain("应用许可证尚未声明。");
  await app.press("第三方声明…");
  await app.waitFor("关闭声明");
  const notices = app.texts().join("\n");
  expect(notices.includes("Rust 依赖许可证清单")).toBe(true);
  expect(notices.includes("nsh-core 0.4.0 — Apache-2.0")).toBe(true);
  expect(notices.includes("Iconoir icons — MIT")).toBe(true);
  expect(notices.includes("unDraw illustrations")).toBe(true);
  expect(notices.includes("将在打包阶段")).toBe(false);
  await app.press("关闭声明");
  await app.waitUntil(() => !app.shows("关闭声明"));
  expect(app.shows("id:about-content")).toBe(true);
});
