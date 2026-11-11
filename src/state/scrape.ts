import { cleanup, signal } from "nsh";
import type { Core } from "haiku/native";

export type ScrapeCandidate = {
  providerId: string; id: string; title: string; artists: string[];
  album?: string; cover?: string; year?: number; trackNo?: number; score: number; sourceCount: number;
};
export type ScrapeItem = {
  track: any; status: string; candidates: ScrapeCandidate[]; automatic: boolean; error?: string;
};
export type ScrapeSnapshot = {
  running: boolean; total: number; completed: number; items: ScrapeItem[];
};
/** Subscribed mirror; job transitions and all metadata changes belong to Rust. */
export function mirrorScrape(core: Core) {
  const snapshot = signal(core.scrape.jobs() as ScrapeSnapshot);
  cleanup(core.scrape.onChange(value => snapshot.set(value as ScrapeSnapshot)));
  return snapshot;
}
