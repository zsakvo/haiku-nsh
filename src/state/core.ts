import { cleanup, resource, signal } from "nsh";
import { Core } from "haiku/native";

export type LibrarySnapshot = {
  revision: number; scanning: boolean;
  progress: { scanned: number; total: number; current: string };
  report: { count: number; skipped: number; errors: string[]; cancelled: boolean };
};
export type TrackSummary = {
  source: string; id: string; title: string; artists: (string | { name: string })[];
  album?: { title: string } | null; cover?: string | null;
  extra?: Record<string, string | number | null> | null;
};
export type PlayerSnapshot = {
  status: string; current?: TrackSummary | null; tracks: TrackSummary[]; index: number;
};
/** Rust owns each snapshot. These signals are read-only mirrors updated by Stop listeners. */
export function createCoreState() {
  const core = resource(async () => Core.open());
  return core;
}
export function mirrorCore(core: Core) {
  const library = signal(core.library.snapshot() as LibrarySnapshot);
  const player = signal(core.player.snapshot() as PlayerSnapshot);
  cleanup(core.library.onProgress((snapshot) => library.set(snapshot as LibrarySnapshot)));
  cleanup(core.library.onChanged((snapshot) => library.set(snapshot as LibrarySnapshot)));
  cleanup(core.player.onState((snapshot) => player.set(snapshot as PlayerSnapshot)));
  return { library, player };
}
