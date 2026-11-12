import { cleanup, resource, signal, Text, View } from "nsh";
import { Core } from "haiku/native";
import { AppShell } from "../../shell/AppShell";
import { Button } from "../../ui/Button";
import type { SettingsSnapshot } from "../../state/settings";

/** Isolated settings and playlists for the small-gap regressions and light previews. */
export function SmallGapsFixture(props: { grid?: boolean; controls?: boolean }) {
  const ready = resource(async () => {
    const core = await Core.openIsolated();
    await core.settings.update({
      themeMode: "light",
      playlistsViewMode: "grid",
      audioEffects: {
        enabled: true,
        presetId: "custom:evening",
        bandGainsDb: [4, 3, 2, 1, 0, 0, 1, 2, 3, 4],
        autoPreamp: false,
        preampDb: -3,
        stereoWidth: 1.4,
        balance: 0.2,
        mono: true,
        customPresets: [{ id: "custom:evening", name: "晚间聆听", gains: Array(10).fill(0) }],
      },
    });
    for (const title of ["周末慢听", "通勤途中", "午后钢琴", "夜色与爵士"]) {
      const id = core.collections.create(title);
      core.collections.add(
        id,
        ["First", "Second", "Third"].map((name, i) => ({
          source: "fixture",
          id: `${title}:${i}`,
          title: name,
          artists: ["Haiku"],
          duration: 180,
        })),
      );
    }
    return core;
  });
  return (
    <View class="flex-1">
      <If when={!!ready()}>
        <AppShell
          initialMode="light"
          initialRoute={props.grid ? { kind: "playlists" } : undefined}
          initialOverlay={props.grid ? undefined : "settings"}
          initialSettingsCategory="播放"
        />
        <If when={props.controls}>
          <Probe core={ready()!} />
        </If>
      </If>
      <If when={!!ready.error()}>
        <Text>{String(ready.error())}</Text>
      </If>
    </View>
  );
}

function Probe(props: { core: Core }) {
  const report = signal("");
  cleanup(
    props.core.settings.onChange((value) => {
      const s = value as SettingsSnapshot;
      const dsp = props.core.player.configuration() as SettingsSnapshot;
      report.set(
        JSON.stringify({
          effects: s.audioEffects,
          dsp: dsp.audioEffects,
          mode: s.playlistsViewMode,
        }),
      );
    }),
  );
  const reopened = signal("");
  return (
    <View class="shrink-0 gap-1">
      <Text testID="small-gaps-report">{report()}</Text>
      <Button
        label="验证视图重开"
        onPress={() =>
          void (async () => {
            const root = props.core.dataDir();
            await props.core.close();
            const core = await Core.openIn(root);
            reopened.set((core.settings.get() as SettingsSnapshot).playlistsViewMode);
          })()
        }
      />
      <Text>{`reopened:${reopened()}`}</Text>
    </View>
  );
}
