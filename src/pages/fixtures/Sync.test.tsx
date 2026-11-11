import { expect, open, test } from "nsh/test";

test("Gist PAT save, create, manual sync and disconnect confirmation", async () => {
  const app = await open("../../../SyncPreview.tsx", {
    platform: "macos", width: 1200, height: 900, network: "live",
  });
  await app.waitFor("id:sync-settings");
  await app.waitFor("text:尚未连接 GitHub");
  await app.type("label:GitHub PAT", "fixture-token");
  await app.press("role:button[保存 PAT]");
  await app.waitFor("text:PAT 已保存。");
  await app.waitFor("text:已保存 PAT · haiku-listener");
  await app.press("role:button[创建私密 Gist]");
  await app.waitFor("text:私密 Gist 已创建。");
  await app.waitFor("text:Gist：abc123");
  await app.press("role:button[立即同步]");
  await app.waitFor("text:同步完成。");
  await app.waitFor("text:版本：fixture-version-2");
  await app.waitUntil(() => app.texts().some(text => text.startsWith("最近同步：")));
  await app.press("role:button[断开同步…]");
  await app.waitFor("text:断开 Gist 同步？");
  await app.press("role:button[取消断开]");
  await app.waitUntil(() => !app.shows("text:断开 Gist 同步？"));
  expect(app.shows("text:Gist：abc123")).toBe(true);
  await app.press("role:button[断开同步…]");
  await app.press("role:button[确认断开]");
  await app.waitFor("text:已断开同步。");
  await app.waitFor("text:尚未连接 GitHub");
  expect(app.shows("text:Gist：abc123")).toBe(false);
});
