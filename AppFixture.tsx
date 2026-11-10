import { mount, resource, Text, View } from "nsh";
import { Core } from "haiku/native";
import { AppShell } from "./src/shell/AppShell";
function Fixture() {
  const core = resource(() => Core.openIsolated());
  return <View class="flex-1">
    <If when={!!core()}><AppShell /></If>
    <If when={!!core.error()}><Text>{String(core.error())}</Text></If>
  </View>;
}
mount(Fixture);
