import { cleanup, signal } from "nsh";
import type { Core } from "haiku/native";

export type Collection = {
  id: string; title: string; description: string; kind: "user" | "liked" | "plugin";
  position: number; trackCount: number; sourceId?: string; sourceLabel?: string;
};
/** Read-only TSX mirror. Mutations, ordering, identities and persistence remain in Rust. */
export function mirrorCollections(core: Core) {
  const collections = signal(core.collections.list() as Collection[]);
  const revision = signal(0);
  cleanup(core.collections.onChange((_change) => {
    collections.set(core.collections.list() as Collection[]);
    revision.update((value) => value + 1);
  }));
  return { collections, revision };
}
