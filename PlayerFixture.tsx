import { mount, resource, signal, Text, View } from "nsh";
import { writeFile } from "fs/promises";
import { Core, LibraryService } from "haiku/native";
import { AppShell } from "./src/shell/AppShell";
import { mirrorCore } from "./src/state/core";
import { Button } from "./src/ui/Button";
import { prepareLibrary, removeLibrary } from "./src/pages/fixtures/libraryAudio";

function Fixture() {
  const ready = resource(async () => {
    const root = `/tmp/haiku-m1-03-fixtures/m1-04-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    await prepareLibrary(root);
    await writeFile(`${root}/0.lrc`, "[00:00]Alpha opening\n[00:10]Alpha middle\n[00:30]Alpha ending");
    await writeFile(`${root}/1.lrc`, "[00:00]Beta opening\n[00:10]Beta middle\n[00:30]Beta ending");
    const core = await Core.openIsolated();
    await core.player.clear();
    await core.player.setMode("sequential");
    await core.player.setVolume(0);
    await core.library.addRoots([root]);
    await core.library.scan();
    const handle = await core.library.query({ search: "Song", sort: "title" });
    const ids = await LibraryService.queryIds(handle);
    const tracks = ids.map((id) => core.library.track(id));
    await core.library.releaseQuery(handle);
    return { root, core, tracks };
  });
  return <View class="flex-1">
    <If when={!!ready()}>
      <AppShell initialMode="light" />
      <Probe {...ready()!} />
    </If>
    <If when={!!ready.error()}><Text>{String(ready.error())}</Text></If>
  </View>;
}
function Probe(props: { core: Core; root: string; tracks: unknown[] }) {
  const state = mirrorCore(props.core);
  const cleaned = signal(false);
  return <View class="flex-row items-center gap-2">
    <Text>{`player:${state.player().status};track:${state.player().current?.title ?? ""}`}</Text>
    <Button label="浅色外观" onPress={() => void props.core.settings.update({ themeMode: "light" })} />
    <Button label="深色外观" onPress={() => void props.core.settings.update({ themeMode: "dark" })} />
    <Button label="装载暂停队列" onPress={() => void (async () => {
      await props.core.player.append(props.tracks);
      await props.core.player.pause();
    })()} />
    <Button label="清理播放夹具" onPress={() => void (async () => {
      await props.core.player.clear();
      await props.core.library.removeRoot(props.root);
      await props.core.library.scan();
      await removeLibrary(props.root);
      cleaned.set(true);
    })()} />
    <If when={cleaned()}><Text>player-fixture-cleaned</Text></If>
  </View>;
}
mount(Fixture);
