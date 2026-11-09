import { expect, open, test } from "nsh/test";

test("S7 empty native background preserves the 64pt strip and NSH overlay input", async () => {
  const app = await open("./GlassFixture.tsx", { width: 1000, height: 700, platform: "macos" });
  await app.waitFor("text:Headless 底栏高度 64");
  await app.press("role:button[底栏按钮]");
  await app.waitFor("text:玻璃底栏 · NSH 内容 1");
  expect(app.texts()).toContain("Headless 底栏高度 64");
});
