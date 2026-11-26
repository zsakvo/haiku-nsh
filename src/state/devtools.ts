import { signal } from "nsh";
import type { Core } from "haiku/native";

/** A redacted plugin HTTP trace event, stamped when the app heard it. */
export interface HttpTrace {
  seq: number; time: number; sourceId: string; phase: "request" | "response";
  method: string; url: string; status?: number; headers: Record<string, string>;
}
export interface PluginWorkspace {
  path: string; name: string; author: string; description: string;
  pluginId: string; createdAt: number; done: boolean; prompt: string;
}
export interface WorkspaceStatus {
  phase: "waiting" | "failed" | "loaded" | "done"; message: string; summary?: string;
  plugin?: { id: string; name: string; version: string; kind: string; capabilities: string[] };
}

/** App-wide so traces keep accumulating while the developer tools window is closed. */
const traces = signal<HttpTrace[]>([]);
let stopCapture: (() => void) | undefined, seq = 0;
export const httpTraces = traces;
export function startTraceCapture(core: Core) {
  if (stopCapture) return;
  stopCapture = core.plugins.onEvent(value => {
    const event = value as Partial<HttpTrace> & { kind?: string };
    if (event.kind !== "http") return;
    traces.set([...traces(), {
      seq: ++seq, time: Date.now(), sourceId: event.sourceId ?? "", phase: event.phase ?? "request",
      method: event.method ?? "GET", url: event.url ?? "", status: event.status ?? undefined, headers: event.headers ?? {},
    }].slice(-500));
  });
}
export function stopTraceCapture() { stopCapture?.(); stopCapture = undefined; }
export function clearTraces() { traces.set([]); }
