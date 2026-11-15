import { mount, resource, Text, View } from "nsh";
import { Core, LibraryService } from "haiku/native";
import { Buffer } from "buffer";
import { writeFile } from "fs/promises";
import { AppShell } from "./src/shell/AppShell";
import { prepareLibrary } from "./src/pages/fixtures/libraryAudio";
import { collectionCover } from "./src/pages/fixtures/collectionCover";

/** Starts real local playback and opens the production overlay without GUI input. */
function Preview() {
  const ready = resource(async () => {
    const core = await Core.openIsolated();
    const root = `/tmp/haiku-m1-03-fixtures/m7-02-preview-${Date.now()}`;
    await prepareLibrary(root);
    await writeFile(`${root}/cover.png`, Buffer.from(collectionCover, "base64"));
    await writeFile(`${root}/0.lrc`, "[00:00]Alpha opening\n[00:10]Alpha middle\n[00:30]Alpha ending");
    await core.library.addRoots([root]);
    await core.library.scan();
    await core.player.setVolume(0);
    await core.player.setMode("repeatAll");
    const source = await core.library.query({ sort: "path" });
    const ids = await LibraryService.queryIds(source);
    for (const id of ids) await core.library.edit([id], { cover: `${root}/cover.png` });
    await core.library.playSource(source, ids[0]);
    await core.library.releaseQuery(source);
    console.log("M7-02 preview playback ready; opening in 5s");
    await new Promise<void>((resolve) => setTimeout(resolve, 5000));
    return true;
  });
  return <View class="flex-1">
    <If when={!!ready()}>
      <AppShell initialMode="dark" initialRoute={{ kind: "library" }} initialNowPlaying />
    </If>
    <If when={!!ready.error()}><Text>{String(ready.error())}</Text></If>
  </View>;
}
mount(Preview);
