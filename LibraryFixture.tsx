import { mount, resource, signal, Text, View } from "nsh";
import { Core } from "haiku/native";
import { AppShell } from "./src/shell/AppShell";
import { mirrorCore } from "./src/state/core";
import { Button } from "./src/ui/Button";
import { prepareLibrary, removeLibrary } from "./src/pages/fixtures/libraryAudio";

// A test entry, sharing the same Core as the real shell. It never touches the normal data root.
function Fixture() {
  const ready = resource(async () => {
    // Headless modules have asset:/ URLs, so the host grants this explicit fixture root.
    const root = `/tmp/haiku-m1-03-fixtures/music-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    await prepareLibrary(root, 120);
    return { root, core: await Core.openIsolated() };
  });
  return <View class="flex-1">
    <If when={!!ready()}>
      <AppShell initialMode="light" initialRoute={{ kind: "library" }}
        injectedLibraryFolders={[ready()!.root]} />
      <Probe core={ready()!.core} root={ready()!.root} />
    </If>
    <If when={!!ready.error()}><Text>{String(ready.error())}</Text></If>
  </View>;
}
function Probe(props: { core: Core; root: string }) {
  const mirrors = mirrorCore(props.core);
  const cleaned = signal(false);
  return <View>
    <Text testID="fixture-player">{`status:${mirrors.player().status};current:${mirrors.player().current?.title ?? ""};queue:${mirrors.player().tracks.map((t) => t.title).join("|")}`}</Text>
    <Button label="清理测试数据" onPress={() => void (async () => {
      await props.core.player.clear();
      for (const root of props.core.library.roots()) await props.core.library.removeRoot(root);
      await props.core.library.scan();
      await removeLibrary(props.root);
      cleaned.set(true);
    })()} />
    <If when={cleaned()}><Text>fixture-cleaned</Text></If>
  </View>;
}
mount(Fixture);
