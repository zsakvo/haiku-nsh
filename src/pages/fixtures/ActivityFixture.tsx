import { resource, signal, Text, View } from "nsh";
import { Core, LibraryService } from "haiku/native";
import { Buffer } from "buffer";
import { writeFile } from "fs/promises";
import { AppShell } from "../../shell/AppShell";
import { Button } from "../../ui/Button";
import { prepareLibrary } from "./libraryAudio";
import { collectionCover } from "./collectionCover";

/** Every invocation owns its Core and media under a new temporary root. */
export function ActivityFixture(props: { page?: "history" | "statistics" | "logs" | "notifications"; controls?: boolean }) {
  const ready = resource(async () => {
    const core = await Core.openIsolated();
    const root = `/tmp/haiku-m1-03-fixtures/m3-02-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    await prepareLibrary(root);
    await writeFile(`${root}/cover.png`, Buffer.from(collectionCover, "base64"));
    await core.library.addRoots([root]);
    await core.library.scan();
    const source = await core.library.query({ sort: "path" });
    const ids = await LibraryService.queryIds(source);
    await core.library.releaseQuery(source);
    const tracks = [];
    for (const id of ids) {
      await core.library.edit([id], { cover: `${root}/cover.png` });
      tracks.push(core.library.track(id));
    }
    await core.player.setVolume(0);
    await core.player.append(tracks);
    await core.player.pause();
    core.collections.setLiked(tracks[0], true);
    if (!props.controls) await core.history.seedFixture(tracks);
    return { core, tracks };
  });
  return <View class="flex-1">
    <If when={!!ready()}>
      <AppShell initialMode="light" initialRoute={{ kind: props.page === "statistics" ? "statistics" : "history" }}
        initialOverlay={props.page === "logs" ? "settings" : undefined} initialSettingsCategory="其他"
        initialLogs={props.page === "logs"} initialNotifications={props.page === "notifications"} />
      <If when={props.controls}><Controls {...ready()!} /></If>
    </If>
    <If when={!!ready.error()}><Text>{String(ready.error())}</Text></If>
  </View>;
}
function Controls(props: { core: Core; tracks: unknown[] }) {
  const status = signal("");
  return <View class="flex-row gap-2">
    <Button label="播放历史夹具" onPress={() => void props.core.player.play([props.tracks[0]], 0)} />
    <Button label="暂停历史夹具" onPress={() => void props.core.player.pause()} />
    <Button label="填充统计夹具" onPress={() => void props.core.history.seedFixture(props.tracks).then(() => status.set("activity-seeded"))} />
    <Text>{status()}</Text>
  </View>;
}
