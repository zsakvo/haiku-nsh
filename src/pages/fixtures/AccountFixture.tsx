import { resource, Text, View } from "nsh";
import { Core } from "haiku/native";
import { AppShell } from "../../shell/AppShell";

/** Isolated credentials and real account/upload services connected exclusively to loopback. */
export function AccountFixture(props: { signedIn?: boolean }) {
  const ready = resource(async () => {
    const core = await Core.openIsolated();
    await core.account.startFixture();
    if (props.signedIn) await core.account.login("listener@example.com", "123456");
    return core;
  });
  return <View class="flex-1">
    <If when={!!ready()}>
      <AppShell initialMode="light" initialOverlay="settings" initialSettingsCategory="账户" />
    </If>
    <If when={!!ready.error()}><Text>{String(ready.error())}</Text></If>
  </View>;
}
