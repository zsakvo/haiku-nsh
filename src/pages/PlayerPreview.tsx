import { cleanup, resource, Text, View } from "nsh";
import { Core, LibraryService } from "haiku/native";
import { AppShell } from "../shell/AppShell";
import type { AppearanceMode } from "../ui/theme";
import type { PlayerSnapshot } from "../state/core";

/** Real-window acceptance entry. Rust reads only the generated media and isolated data root. */
export function PlayerPreview(props: { mode: AppearanceMode; artifacts: string; expanded?: boolean; settings?: boolean }) {
  const ready = resource(async () => {
    const core = await Core.openIsolated();
    await core.player.clear();
    await core.library.addRoots([`${props.artifacts}/media`]);
    const report = await core.library.scan();
    console.log("M1-04 real scan", JSON.stringify(report));
    await core.player.setMode("repeatAll");
    await core.player.setVolume(0.08);
    const handle = await core.library.query({ sort: "title" });
    const ids = await LibraryService.queryIds(handle);
    await core.library.playSource(handle, ids[0]);
    await core.library.releaseQuery(handle);
    return core;
  });
  return <View class="flex-1">
    <If when={!!ready()}>
      <PlaybackLog core={ready()!} />
      <AppShell initialMode={props.mode} initialRoute={{ kind: "library" }} initialNowPlaying={props.expanded}
        initialOverlay={props.settings ? "settings" : undefined}
        onLyricChange={(index) => console.log("M1-04 real lyric cursor", index)} />
    </If>
    <If when={!!ready.error()}><Text>{String(ready.error())}</Text></If>
  </View>;
}
function PlaybackLog(props: { core: Core }) {
  cleanup(props.core.player.onState((value) => {
    const state = value as PlayerSnapshot & { error?: unknown };
    console.log("M1-04 real playback", JSON.stringify({ status: state.status,
      current: state.current?.title, index: state.index, error: state.error }));
  }));
  return <View />;
}
