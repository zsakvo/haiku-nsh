import { expect, open, test } from "nsh/test";

const options = { platform: "macos" as const, width: 1200, height: 800, network: "live" as const };

test("playlist automatic download adds existing tracks to the real download queue", async () => {
  const app = await open("../../../AutoDownloadPreview.tsx", options);
  await app.waitFor("label:自动下载");
  await app.press("label:自动下载");
  await app.waitFor("label:跟随移除");
  await app.press("id:nav-downloads");
  await app.waitFor("id:download-list >> text:潮汐之间");
  await app.waitFor("id:download-list >> text:远方回声");
  await app.waitUntil(() => app.texts().filter(t => t === "已完成").length === 2);
});

test("directory move confirmation updates stored paths and moves the actual files", async () => {
  const app = await open("../../../DirectoryMigrationPreview.tsx", options);
  await app.waitFor("text:更换下载目录？");
  expect(app.shows("role:button[取消更换]")).toBe(true);
  expect(app.shows("role:button[保留原文件]")).toBe(true);
  await app.press("role:button[移动已有文件]");
  await app.waitUntil(() => app.texts().some(t => t.startsWith("目录已更新")));
  await app.press("label:关闭设置");
  await app.press("id:nav-downloads");
  await app.waitUntil(() => app.texts().filter(t => t === "已完成").length === 2);
  await app.press("检查迁移路径");
  await app.waitFor("text:迁移路径与文件已确认");
});
