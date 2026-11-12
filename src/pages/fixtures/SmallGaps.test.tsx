import { expect, open, test } from "nsh/test";
import type { AudioEffects } from "../../state/settings";

const options = {
  platform: "macos" as const,
  width: 1280,
  height: 1000,
  host: { params: { controls: true } },
};

test("EQ overwrite, delete confirmation and full reset retain enabled and custom presets", async () => {
  const app = await open("../../../SmallGapsEffectsPreview.tsx", options);
  await app.waitFor("id:playback-settings");
  const snapshot = () =>
    JSON.parse(app.texts().find((t) => t.startsWith('{"effects":'))!) as {
      effects: AudioEffects;
      dsp: AudioEffects;
    };
  await app.press("覆盖当前预设");
  await app.waitUntil(() => snapshot().effects.customPresets[0].gains[0] === 4);
  await app.press("删除预设…");
  await app.waitFor("text:删除预设？");
  expect(snapshot().effects.customPresets.length).toBe(1);
  await app.press("role:button[取消]");
  expect(snapshot().effects.customPresets.length).toBe(1);
  await app.press("重置音效");
  await app.waitUntil(() => snapshot().effects.presetId === "flat");
  const reset = snapshot().effects;
  expect(reset.enabled).toBe(true);
  expect(reset.customPresets[0].gains[0]).toBe(4);
  expect(reset.bandGainsDb).toEqual(Array(10).fill(0));
  expect(reset.autoPreamp).toBe(true);
  expect(reset.preampDb).toBe(0);
  expect(reset.stereoWidth).toBe(1);
  expect(reset.balance).toBe(0);
  expect(reset.mono).toBe(false);
  expect(snapshot().dsp).toEqual(reset);
  await app.press("label:均衡器预设");
  await app.press("role:option[晚间聆听]");
  await app.press("删除预设…");
  await app.press("确认删除预设");
  await app.waitUntil(() => snapshot().effects.customPresets.length === 0);
  expect(snapshot().effects.bandGainsDb[0]).toBe(4);
});

test("playlist view selection persists across Core reopen", async () => {
  const app = await open("../../../SmallGapsGridPreview.tsx", options);
  await app.waitFor("id:page-playlists");
  await app.press("text:列表");
  await app.waitUntil(() => app.texts().some((t) => t.includes('"mode":"list"')));
  await app.press("验证视图重开");
  await app.waitFor("text:reopened:list");
});
