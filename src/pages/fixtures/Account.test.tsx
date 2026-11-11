import { expect, open, test } from "nsh/test";

test("account code countdown, invalid code, login, member display and logout confirmation", async () => {
  const app = await open("../../../AccountPreview.tsx", {
    platform: "macos", width: 1200, height: 900, network: "live",
  });
  await app.waitFor("id:account-settings");
  await app.type("label:邮箱", "listener@example.com");
  await app.press("id:account-send-code");
  await app.waitFor("text:验证码已发送，请查看邮箱。");
  const countdown = () => app.texts().find(text => /^\d+ 秒后重发$/.test(text));
  await app.waitUntil(() => !!countdown());
  const first = Number(countdown()!.split(" ")[0]);
  await app.waitUntil(() => !!countdown() && Number(countdown()!.split(" ")[0]) < first);
  await app.type("label:验证码", "bad");
  await app.press("role:button[登录]");
  await app.waitUntil(() => app.texts().some(text => text.includes("验证码错误")));
  await app.clear("label:验证码");
  await app.type("label:验证码", "123456");
  await app.press("role:button[登录]");
  await app.waitFor("id:account-profile >> text:林间回声");
  await app.waitFor("text:永久会员");
  await app.waitFor("text:Haiku 永久会员");
  await app.waitFor("role:button[购买即将开放]");
  await app.waitUntil(() => app.texts().some(text => text.startsWith("最近同步：")));
  await app.press("role:button[登出…]");
  await app.waitFor("text:登出 Haiku 账户？");
  await app.press("role:button[取消登出]");
  await app.waitUntil(() => !app.shows("text:登出 Haiku 账户？"));
  expect(app.shows("id:account-profile")).toBe(true);
  await app.press("role:button[登出…]");
  await app.press("role:button[确认登出]");
  await app.waitFor("role:button[登录]");
  await app.waitFor("text:登录后自动同步听歌统计");
  expect(app.shows("id:account-profile")).toBe(false);
});
