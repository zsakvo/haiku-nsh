import { resource, signal, Text, View } from "nsh";
import { Core } from "haiku/native";
import { AppShell } from "../../shell/AppShell";
import type { EntityRef } from "../../shell/navigation";
import { Button } from "../../ui/Button";

/** Automatic collection and directory acceptance use only an isolated Core and loopback. */
export function AutoDownloadFixture(props: { directory?: boolean; enabled?: boolean }) {
  const verified = signal("");
  const ready = resource(async () => {
    const core = await Core.openIsolated();
    await core.settings.update({
      downloadDir: `${core.dataDir()}/downloaded`, downloadCoverMode: "none",
      downloadLyricsMode: "none", downloadFollowPlaylistRemoval: true,
    });
    const source = await core.sources.startNetworkFixture() as { sourceId: string };
    const tracks = [
      { source: source.sourceId, id: "1", title: "潮汐之间", artists: ["林间回声"] },
      { source: source.sourceId, id: "2", title: "远方回声", artists: ["林间回声"] },
    ];
    const id = core.collections.create("随身夜航");
    core.collections.add(id, tracks);
    if (props.enabled) await core.settings.update({ autoDownloadPlaylists: [id] });
    if (props.directory) await core.downloads.enqueue(tracks);
    return { core, id, next: `${core.dataDir()}/relocated` };
  });
  async function verifyPaths() {
    const { core, next } = ready()!;
    await core.downloads.verify();
    const jobs = core.downloads.snapshot() as { path: string | null; status: string }[];
    verified.set(jobs.length === 2 && jobs.every(j => j.path?.startsWith(next + "/"))
      && jobs.every(j => j.status === "completed") ? "迁移路径与文件已确认" : "迁移路径尚未更新");
  }
  return <View class="flex-1 flex-col">
    <If when={!!ready()}>
      <AppShell initialMode="light"
        initialRoute={{ kind: "playlist", ref: ready()!.core.collections.key(ready()!.id) as EntityRef }}
        initialOverlay={props.directory ? "settings" : undefined}
        initialSettingsCategory="下载"
        initialDownloadDirectory={props.directory ? ready()!.next : undefined} />
      <If when={props.directory}><View class="flex-row gap-2">
        <Button label="检查迁移路径" onPress={() => void verifyPaths().catch(e => verified.set(String(e)))} />
        <Text>{verified()}</Text>
      </View></If>
    </If>
    <If when={!!ready.error()}><Text>{String(ready.error())}</Text></If>
  </View>;
}
