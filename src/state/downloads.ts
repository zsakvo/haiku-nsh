import { cleanup, signal } from "nsh";
import type { Core } from "haiku/native";

export interface DownloadItem {
  key: string;
  track: { title: string; artists: (string | { name: string })[] };
  status: string;
  received: number;
  total: number | null;
  error?: string;
  warning?: string;
  quality: string;
  actualQuality?: string;
}

/** Rust owns scheduling and persistence; this signal only mirrors its snapshots. */
export function mirrorDownloads(core: Core) {
  const items = signal(core.downloads.snapshot() as DownloadItem[]);
  cleanup(core.downloads.onChange(value => items.set(value as DownloadItem[])));
  return items;
}
