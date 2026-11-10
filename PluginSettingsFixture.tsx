import { cleanup, mount, resource, signal, Text, View } from "nsh";
import { Core } from "haiku/native";
import { Settings } from "./src/shell/Settings";
import { createAppearance } from "./src/ui/theme";
import { Button } from "./src/ui/Button";

/** Local HTTP + real plugin VMs; test controls expose committed state without mocking the bridge. */
function Fixture() {
  const appearance = createAppearance("light");
  const url = signal(""), commits = signal(0), report = signal("");
  let stop: (() => void) | undefined;
  const ready = resource(async () => {
    const c = await Core.open();
    url.set(await c.plugins.startFixture("subsonic.music"));
    stop = c.plugins.onChange(() => commits.set(commits() + 1));
    return c;
  });
  cleanup(() => { stop?.(); void ready()?.plugins.stopFixtures(); });
  return <View class="flex-1">
    <If when={!!ready()}>
      <Settings open onClose={() => {}} appearance={appearance} initialCategory="来源" />
      <View class="absolute bottom-0 flex-row gap-2">
        <Text testID="fixture-url">{url()}</Text>
        <Text testID="fixture-commits">{`commits:${commits()}`}</Text>
        <Text>{report()}</Text>
        <Button label="停止订阅" onPress={() => { stop?.(); report.set(`stopped:${commits()}`); }} />
        <Button label="读取插件状态" onPress={() => void ready()!.plugins.list().then(snapshot => {
          const state = snapshot as { packages: { id: string; enabled: boolean }[] };
          report.set(`fixture-enabled:${state.packages.find(p => p.id === "settings.fixture")?.enabled}`);
        })} />
        <Button label="读取来源数" onPress={() => void ready()!.sources.list().then(snapshot => {
          const state = snapshot as { sources: unknown[] };
          report.set(`sources:${state.sources.length};commits:${commits()}`);
        })} />
      </View>
    </If>
    <If when={!!ready.error()}><Text>{String(ready.error())}</Text></If>
  </View>;
}
mount(Fixture);
