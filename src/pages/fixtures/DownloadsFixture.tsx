import { cleanup, resource, Text, View } from "nsh";
import { Core } from "haiku/native";
import { AppShell } from "../../shell/AppShell";
import { Button } from "../../ui/Button";
import { prepareLibrary, removeLibrary } from "./libraryAudio";

/** Real downloads in an isolated data root, with explicit loopback source controls. */
export function DownloadsFixture(props: { page?: "library" | "downloads" | "settings" | "retry" }) {
  let root: string | undefined;
  cleanup(() => { if (root) void removeLibrary(root); });
  const ready = resource(async () => {
    const core = await Core.openIsolated();
    root = `/tmp/haiku-m1-03-fixtures/downloads-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    await prepareLibrary(root);
    await core.settings.update({
      downloadDir: `${core.dataDir()}/downloaded`, downloadCoverMode: "none", downloadLyricsMode: "none",
      downloadConcurrency: 2, lyricsAutoSearch: false,
    });
    await core.library.addRoots([root]);
    await core.library.scan();
    const fixture = await core.sources.startNetworkFixture() as { sourceId: string };
    const track = (id: string, title: string) => ({
      source: fixture.sourceId, id, title, artists: ["林间回声"], album: { title: "潮汐与回声" },
    });
    if (props.page === "downloads") {
      await core.downloads.enqueue([
        track("1", "潮汐之间"), track("download-slow", "远方回声"),
      ]);
    } else if (props.page === "retry") {
      await core.downloads.enqueue(track("download-retry", "重试之歌"));
    }
    return { core, source: fixture.sourceId };
  });
  return <View class="flex-1 flex-col">
    <If when={!!ready()}>
      <If when={props.page === "retry"}>
        <View class="flex-row px-5 py-2">
          <Button label="恢复下载来源" onPress={() => void ready()!.core.sources.invoke(
            ready()!.source, "track.get", ["download-recover"], {},
          )} />
        </View>
      </If>
      <AppShell initialMode="light"
        initialRoute={{ kind: props.page === "library" ? "library" : "downloads" }}
        initialOverlay={props.page === "settings" ? "settings" : undefined}
        initialSettingsCategory="下载" />
    </If>
    <If when={!!ready.error()}><Text>{String(ready.error())}</Text></If>
  </View>;
}
