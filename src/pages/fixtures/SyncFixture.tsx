import { resource, Text, View } from "nsh";
import { Core } from "haiku/native";
import { AppShell } from "../../shell/AppShell";

/** The real sync authority, with isolated files, memory credentials and a loopback GitHub API. */
export function SyncFixture(props: { connected?: boolean }) {
  const ready = resource(async () => {
    const core = await Core.openIsolated();
    await core.sync.startFixture();
    if (props.connected) {
      await core.sync.setToken("fixture-token");
      await core.sync.createGist();
      await core.sync.syncNow();
    }
    return core;
  });
  return <View class="flex-1">
    <If when={!!ready()}>
      <AppShell initialMode="light" initialOverlay="settings" initialSettingsCategory="同步" />
    </If>
    <If when={!!ready.error()}><Text>{String(ready.error())}</Text></If>
  </View>;
}
