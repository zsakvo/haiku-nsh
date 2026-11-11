import { expect, open, test } from "nsh/test";
const options = { platform: "macos" as const, width: 1200, height: 800, network: "live" as const };

test("library menu downloads a track and removal requires explicit confirmation", async () => {
  const app = await open("../../../DownloadLibraryPreview.tsx", options);
  await app.waitFor("label:Song Alpha");
  await app.rightClick("label:Song Alpha");
  await app.press("role:menuitem[下载]");
  await app.press("id:nav-downloads");
  await app.waitFor("id:download-list >> text:Song Alpha");
  await app.waitFor("text:已完成");
  await app.press("label:移除 Song Alpha");
  await app.waitFor("text:移除下载？");
  await app.press("取消移除");
  expect(app.shows("id:download-list >> text:Song Alpha")).toBe(true);
  await app.press("label:移除 Song Alpha");
  await app.press("确认移除");
  await app.waitFor("text:没有下载任务");
});

test("failed download retries after loopback source recovers", async () => {
  const app = await open("../../../DownloadRetryPreview.tsx", options);
  // The service deliberately exhausts its production 2s/8s automatic retry budget.
  await app.waitFor("text:失败", { timeout: 20000 });
  await app.press("恢复下载来源");
  await app.press("label:重试 重试之歌");
  await app.waitFor("text:已完成");
  expect(app.shows("label:显示文件 重试之歌")).toBe(true);
  await app.press("label:移除 重试之歌");
  await app.press("label:同时删除音频与附加文件");
  await app.press("确认移除");
  await app.waitFor("text:没有下载任务");
});

test("download preferences are saved through the settings service", async () => {
  const app = await open("../../../DownloadSettingsPreview.tsx", options);
  await app.waitFor("id:download-settings");
  await app.clear("label:文件名模板");
  await app.type("label:文件名模板", "{title}");
  await app.press("保存模板");
  await app.press("label:下载音质");
  await app.press("role:option[高品质]");
  await app.press("label:关闭设置");
  await app.keyboard.press("mod+,");
  await app.waitFor("id:download-settings");
  expect(app.shows("text:高品质")).toBe(true);
});
