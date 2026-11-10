import { cleanup, host, mount, resource, signal, Text, View } from "nsh";
import { Core } from "haiku/native";
import { AppShell } from "./src/shell/AppShell";
import { Button } from "./src/ui/Button";
function Fixture() {
  const report = signal("");
  function read(c: Core) {
    const s = c.settings.get() as { themeMode: string; brandColor: string; audioEffects: { bandGainsDb: number[] } };
    const p = c.player.configuration() as typeof s;
    report.set(`persist:${s.themeMode}:${s.brandColor};eq:${s.audioEffects.bandGainsDb[0]};dsp:${p.audioEffects.bandGainsDb[0]}`);
  }
  let stop: (() => void) | undefined;
  cleanup(() => stop?.());
  const core = resource(async () => { const c = typeof host.params.dataRoot === "string" ? await Core.openIn(host.params.dataRoot) : await Core.openIsolated(); stop = c.settings.onChange(() => read(c)); return c; });
  return <View class="flex-1">
    <If when={!!core()}><AppShell initialOverlay="settings" /></If>
    <If when={!!core()}><Text>{`data-root:${core()!.dataDir()}`}</Text></If>
    <Button label="读取接线配置" onPress={() => read(core()!)} />
    <Text>{report()}</Text>
  </View>;
}
mount(Fixture);
