import { expect, open, test } from "nsh/test";

test("shows Haiku and the version returned by Rust", async () => {
  const app = await open("./App.tsx", { platform: "macos" });
  expect(app.texts()).toContain("Haiku");
  expect(app.texts()).toContain("Core 0.1.0");
});

test("presses the echo button and shows the plugin result", async () => {
  const app = await open("./App.tsx", { platform: "macos" });
  await app.press("role:button[验证插件回声]");
  await app.waitFor("text:Plugin echo(21) = 42", { timeout: 15_000 });
});
