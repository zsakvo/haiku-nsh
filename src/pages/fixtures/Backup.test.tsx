import { expect, open, test } from "nsh/test";

test("backup export warning, inspect, cancel and confirmed hot restore", async () => {
  const app = await open("../../../BackupPreview.tsx", {
    platform: "macos", width: 1200, height: 900,
  });
  await app.waitFor("id:backup-settings");
  await app.press("role:button[导出备份…]");
  await app.waitFor("text:包含明文登录凭据");
  await app.press("role:button[确认导出…]");
  await app.waitFor("id:backup-export-result");
  await app.press("role:button[导入备份…]");
  await app.waitFor("text:恢复备份预览");
  await app.waitFor("text:确认后将覆盖对应设置、歌单和插件数据。");
  await app.press("role:button[取消恢复]");
  await app.press("role:button[导入备份…]");
  await app.press("role:button[确认覆盖并恢复]");
  await app.waitFor("id:backup-import-result >> text:备份已恢复");
  await app.press("label:关闭设置");
  await app.waitFor("text:备份中的歌单");
  expect(app.shows("text:恢复前的歌单")).toBe(false);
  await app.keyboard.press("cmd+shift+,");
  await app.waitFor("id:settings-heading");
});
