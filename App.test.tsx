import { expect, open, test } from "nsh/test";
import { destinations } from "./src/shell/navigation";

const options = { platform: "macos" as const, width: 1200, height: 800 };
test("sidebar reaches every destination", async () => {
  const app = await open("./AppFixture.tsx", options);
  await app.waitFor("id:nav-home");
  for (const page of destinations) {
    await app.press(`id:nav-${page.id}`);
    await app.waitFor(["home", "categories", "radio", "search", "history", "statistics"].includes(page.id) ? `id:page-${page.id}` : `text:${page.title}`);
    if (!["home", "categories", "radio", "search", "favorites", "playlists", "history", "statistics"].includes(page.id)) expect(app.texts()).toContain(page.description);
  }
});
test("back and forward retain page input; new navigation truncates forward history", async () => {
  const app = await open("./AppFixture.tsx", options);
  await app.waitFor("id:nav-home");
  await app.press("id:nav-search");
  await app.type("label:搜索词", "坂本龙一");
  await app.press("id:nav-library");
  await app.keyboard.press("mod+[");
  expect(app.value("label:搜索词")).toBe("坂本龙一");
  await app.keyboard.press("mod+]");
  expect(app.texts()).toContain("你的音乐，放在这里");
  await app.keyboard.press("mod+[");
  await app.press("id:nav-favorites");
  await app.keyboard.press("mod+]");
  expect(app.texts()).toContain("收藏值得重听的音乐");
});
test("Cmd+K opens the real Command and navigates", async () => {
  const app = await open("./AppFixture.tsx", options);
  await app.waitFor("id:nav-home");
  await app.keyboard.press("mod+k");
  await app.type("label:查找命令", "下载");
  await app.keyboard.press("enter");
  await app.waitFor("text:让音乐随时相伴");
});
test("settings theme changes live; Escape preserves original page and history", async () => {
  const app = await open("./AppFixture.tsx", options);
  await app.waitFor("id:nav-home");
  await app.press("id:nav-search");
  await app.type("label:搜索词", "保留的搜索词");
  await app.press("id:nav-library");
  await app.keyboard.press("mod+,");
  await app.waitFor("id:settings-heading");
  await app.press("role:radio[浅色]");
  const light = app.spanStyle("text:让 Haiku 更像你").color;
  await app.press("role:radio[深色]");
  await app.waitUntil(() => app.spanStyle("text:让 Haiku 更像你").color !== light);
  await app.press("label:品牌色：蓝");
  await app.keyboard.press("escape");
  await app.waitUntil(() => !app.shows("id:settings-heading"));
  expect(app.texts()).toContain("你的音乐，放在这里");
  await app.keyboard.press("mod+[");
  expect(app.value("label:搜索词")).toBe("保留的搜索词");
  await app.keyboard.press("mod+]");
  expect(app.texts()).toContain("你的音乐，放在这里");
});
