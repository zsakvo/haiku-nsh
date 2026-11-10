import { cleanup, resource, signal } from "nsh";
import { Core } from "haiku/native";

export interface PluginField {
  label: string; type: string; required?: boolean; placeholder?: string;
}
export interface PluginDescriptor {
  id: string; name: string; version: string; author?: string; logo?: string;
  builtin: boolean; enabled: boolean; kind: string; capabilities: string[]; authMethods: string[];
  connection?: { fields: Record<string, PluginField> };
  auth?: {
    password?: { fields?: Record<string, PluginField> };
    qrcode?: { variants?: Record<string, string> };
    web?: { url: string; userAgent?: string };
  };
}
export interface SourceRecord {
  sourceId: string; pluginId: string; displayName?: string; enabled: boolean;
  needsLogin?: boolean; user?: { name: string }; connection?: Record<string, string>;
}
export interface PluginSnapshot {
  packages: PluginDescriptor[]; sources: SourceRecord[]; errors: Record<string, unknown>;
}
/** The signals mirror Rust commits; drafts and selection belong to the current surface. */
export function createPlugins() {
  const value = signal<PluginSnapshot>({ packages: [], sources: [], errors: {} });
  const events = signal<unknown[]>([]), error = signal(""), busy = signal(false);
  let stop: (() => void) | undefined, stopEvents: (() => void) | undefined;
  let closed = false;
  cleanup(() => { closed = true; stop?.(); stopEvents?.(); });
  const core = resource(async () => {
    const c = await Core.open();
    if (closed) return c;
    stop = c.plugins.onChange(next => value.set(next as PluginSnapshot));
    stopEvents = c.plugins.onEvent(event => events.set([...events(), event].slice(-100)));
    value.set(await c.plugins.list() as PluginSnapshot);
    return c;
  });
  async function run<T>(work: (c: Core) => Promise<T>): Promise<T | undefined> {
    busy.set(true); error.set("");
    try { return await work(core() ?? await Core.open()); }
    catch (e) { error.set(String(e)); }
    finally { busy.set(false); }
  }
  return { value, events, error, busy, core, run };
}
export type PluginsState = ReturnType<typeof createPlugins>;
