import { cleanup, signal } from "nsh";
import type { Core } from "haiku/native";
export type ShortcutBinding = { id: string; scope: "app" | "global"; chord: string };
export type ShortcutSnapshot = { supported: boolean; bindings: ShortcutBinding[]; defaults: ShortcutBinding[]; labels: Record<string, string> };
/** Rust owns validation, persistence and live registrations; this is a subscribed mirror. */
export function createShortcuts(core: Core) {
  const value = signal(core.shortcuts.snapshot() as ShortcutSnapshot);
  cleanup(core.shortcuts.onChange((s) => value.set(s as ShortcutSnapshot)));
  return { value, service: core.shortcuts };
}
/** Reference spellings are kept on disk; NSH uses shorter arrow names. */
export function nshChord(chord: string) { return chord.replace(/arrow/g, "").replace(/control/g, "ctrl").replace(/meta/g, "cmd").replace(/\+=$/, "+plus"); }
