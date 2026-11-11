import { cleanup, keymap, signal, Text, View } from "nsh";
import type { Core } from "haiku/native";
import { Button } from "../ui/Button";
import { createShortcuts, nshChord } from "../state/shortcuts";

/** A recorder has its own owner, so every temporary key binding ends on cancel or completion. */
function ShortcutRecorder(props: { onRecord: (chord: string) => void; onCancel: () => void }) {
  // NSH exposes command keymaps rather than a raw keydown callback. Cover the same
  // physical keys as Rust validation; platform-reserved combinations cannot be recorded.
  const keys = [..."abcdefghijklmnopqrstuvwxyz0123456789", "[", "]", ",", ".", "/", ";", "'", "\\", "`", "plus", "minus", "space", "enter", "tab", "backspace", "delete", "left", "right", "up", "down", ...Array.from({ length: 20 }, (_, i) => `f${i + 1}`)];
  for (let mask = 0; mask < 16; mask++) {
    const mods = ["cmd", "ctrl", "alt", "shift"].filter((_, i) => mask & (1 << i));
    for (const key of keys) {
      const chord = [...mods, key].join("+");
      try { cleanup(keymap.bind(chord, () => props.onRecord(chord), { when: "always" })); }
      catch { /* Platform-owned keys remain owned by the OS. */ }
    }
  }
  cleanup(keymap.bind("escape", props.onCancel, { when: "always" }));
  return <Text class="text-xs text-muted-foreground">按下组合键，Esc 取消</Text>;
}
export function ShortcutSettings(props: { core: Core; initialConflict?: string }) {
  const state = createShortcuts(props.core);
  const recording = signal<string | null>(null), busy = signal(false), error = signal(props.initialConflict ?? "");
  async function act(action: () => Promise<unknown>) {
    if (busy()) return;
    busy.set(true);
    try { await action(); error.set(""); recording.set(null); }
    catch (e) { error.set(e instanceof Error ? e.message : String(e).replace(/^Error:\s*/, "")); recording.set(null); }
    finally { busy.set(false); }
  }
  function record(chord: string) {
    const target = recording(); if (!target) return;
    void act(() => state.service.apply(state.value().bindings.map((b) => `${b.scope}:${b.id}` === target ? { ...b, chord } : b)));
  }
  return <View testID="shortcut-settings" class="gap-4">
    <View class="flex-row items-center gap-3">
      <Text role="header" class="flex-1 text-xl font-semibold text-foreground">快捷键</Text>
      <Button testID="shortcut-reset-all" label="全部恢复默认" size="sm" disabled={busy()} onPress={() => void act(() => state.service.resetAll())} />
    </View>
    <If when={!!error()}><Text testID="shortcut-error" class="text-sm text-destructive">{error()}</Text></If>
    <If when={!!recording()}><ShortcutRecorder onRecord={record} onCancel={() => recording.set(null)} /></If>
    <Each of={state.value().supported ? ["app", "global"] : ["app"]} key={(scope) => scope}>{(scope) => <View class="gap-2">
      <Text role="header" class="text-base font-semibold text-foreground">{scope() === "app" ? "应用内" : "全局"}</Text>
      <Text class="text-xs text-muted-foreground">{scope() === "app" ? "主窗口内生效。播放快捷键在文本输入时停用。" : "其他应用在前台时也生效。普通键需要 Cmd、Ctrl 或 Option；F1–F20 可单独使用。"}</Text>
      <Each of={state.value().bindings.filter((b) => b.scope === scope())} key={(b) => b.id}>{(binding) => <View class="min-h-8 flex-row items-center gap-3 border-b border-border py-1">
        <Text class="flex-1 text-sm text-foreground">{state.value().labels[binding().id]}</Text>
        <Button testID={`shortcut-edit-${scope()}-${binding().id}`} size="sm" class="min-w-32"
          label={recording() === `${scope()}:${binding().id}` ? "按下快捷键…" : binding().chord ? keymap.label(nshChord(binding().chord)) : "未绑定"}
          disabled={busy()} onPress={() => { error.set(""); recording.set(`${scope()}:${binding().id}`); }} />
        <Button testID={`shortcut-reset-${scope()}-${binding().id}`} label="恢复默认" size="sm" variant="ghost" disabled={busy()}
          onPress={() => void act(() => state.service.resetOne(`${scope()}:${binding().id}`))} />
      </View>}</Each>
    </View>}</Each>
  </View>;
}
