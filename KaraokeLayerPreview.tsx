import { mount, resource, Text, View } from "nsh";
import { Core, LibraryService } from "haiku/native";
import { Buffer } from "buffer";
import { writeFile } from "fs/promises";
import { AppShell } from "./src/shell/AppShell";
import { prepareLibrary } from "./src/pages/fixtures/libraryAudio";
import { collectionCover } from "./src/pages/fixtures/collectionCover";

/** Real enhanced-LRC playback; opens automatically for capture without GUI input. */
function Preview() {
  const ready = resource(async () => {
    const core = await Core.openIsolated();
    const root = `/tmp/haiku-m1-03-fixtures/m7-03-preview-${Date.now()}`;
    await prepareLibrary(root);
    await writeFile(`${root}/cover.png`, Buffer.from(collectionCover, "base64"));
    const lrc = Array.from({ length: 12 }, (_, i) => {
      const stamp = (seconds: number) => `00:${String(seconds).padStart(2, "0")}.00`;
      const start = i * 5;
      return `[${stamp(start)}]<${stamp(start)}>原生<${stamp(start + 1)}>逐字歌词 <${stamp(start + 2)}>CoreText <${stamp(start + 3)}>流动的光<${stamp(start + 5)}>`;
    }).join("\n");
    for (const i of [0, 1, 2]) await writeFile(`${root}/${i}.lrc`, lrc);
    await core.library.addRoots([root]);
    await core.library.scan();
    await core.player.setVolume(0);
    await core.player.setMode("repeatOne");
    const source = await core.library.query({ sort: "path" });
    const ids = await LibraryService.queryIds(source);
    for (const id of ids) await core.library.edit([id], { cover: `${root}/cover.png` });
    await core.library.playSource(source, ids[0]);
    await core.library.releaseQuery(source);
    console.log("M7-03 enhanced-LRC playback ready; opening Now Playing");
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
