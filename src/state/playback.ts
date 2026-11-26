import type { Core } from "haiku/native";

/** Play one track by inserting it after the current entry, keeping the rest of the queue. */
export async function playInserted(core: Core, track: unknown) {
  const s = core.player.snapshot() as { tracks?: unknown[]; index?: number };
  if (!s.tracks?.length || (s.index ?? -1) < 0) { await core.player.play([track], 0); return; }
  await core.player.playNext(track);
  await core.player.next();
}
