import { cleanup, resource, Text, View } from "nsh";
import { readFile } from "fs/promises";
import { Core, LibraryService } from "haiku/native";
import { AppShell } from "../shell/AppShell";
import type { AppearanceMode } from "../ui/theme";
import type { PlayerSnapshot } from "../state/core";
import { prepareLibrary } from "./fixtures/libraryAudio";

/** Fixed real-window acceptance preview. Audio and application data stay under .artifacts. */
export function LibraryPreview(props: { mode: AppearanceMode; artifacts: string }) {
  const ready = resource(async () => {
    const root = `${props.artifacts}/preview-music-${props.mode}`;
    await prepareLibrary(root, 0, await readFile(`${props.artifacts}/audio/fixture.flac`));
    const core = await Core.open();
    await core.library.addRoots([root]);
    const report = await core.library.scan();
    console.log("M1-03 real scan", JSON.stringify({ root, report }));
    await core.player.setMode("sequential");
    await core.player.setVolume(0.08);
    const handle = await core.library.query({ search: "Song", sort: "title" });
    const ids = await LibraryService.queryIds(handle);
    await core.library.playSource(handle, ids[1]);
    await core.library.releaseQuery(handle);
    return core;
  });
  return <View class="flex-1">
    <If when={!!ready()}>
      <PlaybackLog core={ready()!} />
      <AppShell initialMode={props.mode} initialRoute={{ kind: "library" }} />
    </If>
    <If when={!!ready.error()}><Text>{String(ready.error())}</Text></If>
  </View>;
}
function PlaybackLog(props: { core: Core }) {
  cleanup(props.core.player.onState((value) => {
    const state = value as PlayerSnapshot & { error?: unknown };
    console.log("M1-03 real playback", JSON.stringify({
      status: state.status, current: state.current?.title,
      queue: state.tracks.map((t) => t.title), index: state.index, error: state.error,
    }));
  }));
  return <View />;
}
