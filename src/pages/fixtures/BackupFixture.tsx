import { resource, Text, View } from "nsh";
import { Core } from "haiku/native";
import { AppShell } from "../../shell/AppShell";

/** All archives, credentials and restore targets belong to this isolated Core. No network. */
export function BackupFixture(props: { dialog?: "export" | "import" }) {
  const ready = resource(async () => {
    const core = await Core.openIsolated();
    const id = core.collections.create("备份中的歌单");
    const bindings = (core.shortcuts.snapshot() as { bindings: { id: string; scope: string; chord: string }[] }).bindings;
    await core.shortcuts.apply(bindings.map(b => b.scope === "app" && b.id === "openSettings"
      ? { ...b, chord: "meta+shift+," } : b));
    const path = `${core.dataDir()}/fixture.haikubak`;
    await core.backup.export(path, { includeGithubToken: false });
    await core.shortcuts.resetAll();
    core.collections.rename(id, "恢复前的歌单", "");
    return { path };
  });
  return <View class="flex-1">
    <If when={!!ready()}>
      <AppShell initialMode="light" initialOverlay="settings" initialSettingsCategory="其他"
        backupPath={ready()!.path} initialBackupDialog={props.dialog} />
    </If>
    <If when={!!ready.error()}><Text>{String(ready.error())}</Text></If>
  </View>;
}
