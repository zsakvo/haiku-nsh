import { cleanup, resource, Text, View } from "nsh";
import { Core, LibraryService } from "haiku/native";
import { AppShell } from "../shell/AppShell";
/** Real-window M1-05 acceptance: ordinary Core commands, no synthesized GUI input. */
export function EffectsSettingsPreview(props: { artifacts: string; category: string }) {
  const ready = resource(async () => {
    const core = await Core.open();
    await core.settings.update({ themeMode: "light", brandColor: "orange", audioEffects: { enabled: true, presetId: "vocal", bandGainsDb: [-2,-2,-1,2,4,4,3,1,0,-1] } });
    await core.player.clear();
    await core.library.addRoots([`${props.artifacts}/media`]);
    console.log("M1-05 real scan", JSON.stringify(await core.library.scan()));
    await core.player.setVolume(0.08);
    const source = await core.library.query({ sort: "title" });
    const ids = await LibraryService.queryIds(source);
    await core.library.playSource(source, ids[0]);
    await core.library.releaseQuery(source);
    return core;
  });
  return <View class="flex-1">
    <If when={!!ready()}>
      <PreviewLog core={ready()!} />
      <AppShell initialOverlay="settings" initialSettingsCategory={props.category} />
    </If>
    <If when={!!ready.error()}><Text>{String(ready.error())}</Text></If>
  </View>;
}
function PreviewLog(props: { core: Core }) {
  cleanup(props.core.player.onState((state) => console.log("M1-05 playback", JSON.stringify(state))));
  const pause = setTimeout(() => void props.core.player.pause().catch(console.error), 15000);
  cleanup(() => clearTimeout(pause));
  return <View />;
}
