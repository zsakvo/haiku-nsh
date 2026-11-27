import { signal } from "nsh";
import type { Core } from "haiku/native";

/** One plugin HTTP exchange: the request, then its response or failure, merged by request id. */
export interface HttpExchange {
  id: number; time: number; sourceId: string; method: string; url: string;
  state: "pending" | "done" | "failed";
  status?: number; elapsedMs?: number; error?: string;
  requestHeaders: Record<string, string>; responseHeaders: Record<string, string>;
  requestBody?: string; responseBody?: string; requestSize?: number; responseSize?: number;
}
export interface PluginWorkspace {
  path: string; name: string; author: string; website?: string; description: string;
  pluginId: string; createdAt: number; done: boolean; prompt: string; logo?: string | null;
}
export interface WorkspaceStatus {
  phase: "waiting" | "writing" | "failed" | "loaded" | "done"; message: string; summary?: string;
  plugin?: { id: string; name: string; version: string; kind: string; capabilities: string[] };
}

interface HttpEvent {
  kind?: string; id?: number; sourceId?: string; phase?: "request" | "response" | "error";
  method?: string; url?: string; status?: number | null; headers?: Record<string, string>;
  body?: string | null; size?: number | null; elapsedMs?: number | null; error?: string | null;
}

const LIMIT = 300;
const exchanges = signal<HttpExchange[]>([]);
let stop: (() => void) | undefined;
export const httpExchanges = exchanges;

function merge(event: HttpEvent) {
  const id = event.id ?? 0;
  const list = exchanges();
  const index = list.findIndex((x) => x.id === id);
  if (event.phase === "request" || index < 0) {
    if (event.phase !== "request") return;
    exchanges.set([...list, {
      id, time: Date.now(), sourceId: event.sourceId ?? "", method: event.method ?? "GET", url: event.url ?? "",
      state: "pending" as const, requestHeaders: event.headers ?? {}, responseHeaders: {},
      requestBody: event.body ?? undefined, requestSize: event.size ?? undefined,
    }].slice(-LIMIT));
    return;
  }
  const next = list.slice();
  next[index] = event.phase === "response"
    ? { ...list[index], state: "done", status: event.status ?? undefined, url: event.url || list[index].url,
      responseHeaders: event.headers ?? {}, responseBody: event.body ?? undefined,
      responseSize: event.size ?? undefined, elapsedMs: event.elapsedMs ?? undefined }
    : { ...list[index], state: "failed", error: event.error ?? "请求失败", elapsedMs: event.elapsedMs ?? undefined };
  exchanges.set(next);
}

/** Record only while the developer tools window is open: the host traces HTTP only while
 * someone listens to `onEvent`, and this is the only listener. */
export function startHttpCapture(core: Core) {
  if (stop) return;
  stop = core.plugins.onEvent((value) => {
    const event = value as HttpEvent;
    if (event.kind === "http") merge(event);
  });
}
/** Stop recording and drop what was recorded. */
export function stopHttpCapture() {
  stop?.(); stop = undefined;
  exchanges.set([]);
}
export function clearHttpExchanges() { exchanges.set([]); }
