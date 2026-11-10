import { expect, test } from "nsh/test";
import { Core } from "haiku/native";

// Each test owns a Core directory, independent of test order and process environment.
test("Core player queue commands publish low-frequency state and Stop unregisters", async () => {
  const core = await Core.openIsolated();
  expect((await Core.open()).player === core.player).toBe(true);
  const player = core.player;
  const heard: any[] = [];
  const stop = player.onState((state) => heard.push(state));
  const stateAfter = (predicate: (state: any) => boolean): Promise<any> => new Promise((resolve) => {
    const off = player.onState((state) => { if (predicate(state)) { off(); resolve(state); } });
  });
  const track = (id: string) => ({ source: "fixture#source", id, title: id, artists: [], localPath: `/unused/${id}.flac` });
  try {
    await player.clear();
    await player.append([track("one"), track("two")]);
    const appended = await stateAfter((state) => state.tracks.length === 2);
    expect(appended.index).toBe(0);
    expect(appended.status).toBe("idle");
    expect("position" in appended).toBe(false);
    expect("buffered" in appended).toBe(false);
    await player.playNext(track("next"));
    await stateAfter((state) => state.tracks.length === 3);
    await player.moveItem(1, 2);
    const moved = await stateAfter((state) => state.tracks[2]?.id === "next");
    expect(moved.current.id).toBe("one");
    await player.remove(1);
    await player.setMode("shuffle");
    await player.setVolume(0.4);
    await player.setSleep(15);
    const sleeping = await stateAfter((state) => state.volume === 0.4 && state.sleep.remainingMs === 900000);
    expect(sleeping.mode).toBe("shuffle");
    await player.extendSleep();
    await stateAfter((state) => state.sleep.remainingMs > 900000);
    await player.cancelSleep();
    await player.pause();
    const paused = await stateAfter((state) => state.status === "paused");
    expect(paused.sleep.afterTrack).toBe(false);
    expect(paused.error).toBe(null);
    let code: unknown;
    try { await player.jump(999); } catch (error) { code = (error as { code?: string }).code; }
    expect(code).toBe("INVALID_ARGUMENT");
    stop();
    const count = heard.length;
    await player.clear();
    await stateAfter((state) => state.tracks.length === 0);
    expect(heard.length).toBe(count);
    await player.flush();
  } finally { stop(); await player.clear(); await core.close(); }
});
