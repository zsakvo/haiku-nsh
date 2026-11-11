import { expect, open, test } from "nsh/test";

const options = { platform: "macos" as const, width: 1200, height: 800 };
test("SeekBar primary threshold, independent preview, one release commit and clamping", async () => {
  const app = await open("./SeekBarFixture.tsx", options);
  await app.waitFor("id:seek-track");
  await app.drag("id:seek-track", { x: 0.5 });
  expect(app.texts()).toContain("commits:0;last:-1.000");
  await app.press("推进行钟");
  await app.dragAndHold("id:seek-track", { x: 40 });
  expect(app.texts()).toContain("1:00"); // middle + 40 / 400, duration 100
  expect(app.texts()).toContain("commits:0;last:-1.000");
  await app.wait(300);
  expect(app.texts()).toContain("1:00");
  await app.drop();
  expect(app.texts()).toContain("commits:1;last:60.000");
  await app.rightClick("id:seek-track");
  expect(app.texts()).toContain("commits:1;last:60.000");
  await app.drag("id:seek-track", { x: 1000 });
  expect(app.texts()).toContain("commits:2;last:100.000");
  await app.press("未知时长");
  await app.drag("id:seek-track", { x: -50 });
  expect(app.texts()).toContain("commits:2;last:100.000");
});

test("persistent dock, commands through Core, retained navigation and local lyric switching", async () => {
  const app = await open("./PlayerFixture.tsx", options);
  try {
    await app.waitFor("id:player-dock-1");
    expect(app.enabled("label:播放 / 暂停")).toBe(false);
    expect(app.enabled("label:下一首")).toBe(false);
    await app.press("装载暂停队列");
    await app.waitFor("text:player:paused;track:Song Alpha");
    expect(app.spanStyle("id:dock-title >> text:/^.+$/").color).toBe("#272522ff");
    expect(app.spanStyle("id:seek-time >> text:/^.+$/").color).toBe("#716b64ff");
    expect(app.spanStyle("id:seek-duration >> text:/^.+$/").color).toBe("#716b64ff");
    await app.press("深色外观");
    await app.waitUntil(() => app.spanStyle("id:dock-title >> text:/^.+$/").color === "#f2efebff");
    expect(app.spanStyle("id:seek-time >> text:/^.+$/").color).toBe("#aaa298ff");
    await app.press("浅色外观");
    await app.waitUntil(() => app.spanStyle("id:dock-title >> text:/^.+$/").color === "#272522ff");
    await app.press("id:nav-search");
    await app.type("label:搜索词", "保留的播放页状态");
    await app.press("id:nav-library");
    await app.press("label:显示歌词");
    await app.waitFor("id:now-playing");
    // Now Playing is a dark surface even while the application is in light mode.
    expect(app.spanStyle("id:dock-title >> text:/^.+$/").color).toBe("#f9f7f4ff");
    expect(app.spanStyle("id:dock-artist >> text:/^.+$/").color).toBe("#d6d1caff");
    expect(app.spanStyle("id:seek-time >> text:/^.+$/").color).toBe("#d6d1caff");
    expect(app.spanStyle("id:seek-duration >> text:/^.+$/").color).toBe("#d6d1caff");
    await app.waitFor("label:Alpha middle"); // lyrics load while paused, before any audio open
    expect(app.count("id:player-dock-1")).toBe(1);
    await app.press("label:Alpha middle");
    await app.waitFor("text:0:10");
    await app.keyboard.press("escape");
    await app.waitUntil(() => !app.shows("id:now-playing"));
    expect(app.spanStyle("id:dock-title >> text:/^.+$/").color).toBe("#272522ff");
    expect(app.spanStyle("id:seek-time >> text:/^.+$/").color).toBe("#716b64ff");
    await app.keyboard.press("mod+[");
    expect(app.value("label:搜索词")).toBe("保留的播放页状态");
    await app.keyboard.press("mod+]");
    expect(app.shows("id:page-library")).toBe(true);
    await app.keyboard.press("mod+,");
    await app.waitFor("id:settings-heading");
    expect(app.count("id:player-dock-1")).toBe(1);
    expect(app.enabled("label:播放 / 暂停")).toBe(true);
    await app.press("label:播放 / 暂停");
    await app.waitFor("text:player:playing;track:Song Alpha");
    await app.press("label:播放 / 暂停");
    await app.waitFor("text:player:paused;track:Song Alpha");
    await app.keyboard.press("escape");
    await app.waitUntil(() => !app.shows("id:settings-heading"));
    await app.press("label:下一首");
    await app.waitFor("text:player:playing;track:Song Beta");
    await app.press("label:播放 / 暂停");
    await app.waitFor("text:player:paused;track:Song Beta");
    await app.press("label:打开正在播放");
    await app.waitFor("label:Beta middle");
    expect(app.shows("label:Alpha middle")).toBe(false);
    expect(app.count("id:player-dock-1")).toBe(1);
    await app.keyboard.press("escape");
    await app.waitUntil(() => !app.shows("id:now-playing"));
  } finally {
    await app.press("清理播放夹具");
    await app.waitFor("text:player-fixture-cleaned");
  }
});
