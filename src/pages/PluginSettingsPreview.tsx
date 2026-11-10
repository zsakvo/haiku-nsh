import { cleanup, resource, Text, View } from "nsh";
import { Core } from "haiku/native";
import { AppShell } from "../shell/AppShell";

/** Real-window evidence uses the loopback HTTP fixture and an actual capability invocation. */
export function PluginSettingsPreview(props: { category: string; login?: boolean }) {
  const ready = resource(async () => {
    const c = await Core.open();
    await c.settings.update({ themeMode: "light" });
    const url = await c.plugins.startFixture("subsonic.music");
    await c.sources.connect("subsonic.music", {
      baseUrl: url, username: "fixture", password: "fake-password",
    }, "Subsonic 离线音乐库", null);
    if (props.category === "来源" && !props.login) {
      await c.sources.connect("subsonic.music", {
        baseUrl: url, username: "archive", password: "fake-password",
      }, "Subsonic 离线音乐库", null);
    }
    const state = await c.sources.list() as {
      sources: { sourceId: string; connection?: { baseUrl?: string } }[];
    };
    const source = state.sources.find(source => source.connection?.baseUrl === url)!;
    const result = await c.plugins.invoke(source.sourceId, "track.get", ["song:1/字"], {});
    console.log("M2-04 preview ready", props.category);
    return { core: c, result };
  });
  cleanup(() => { void ready()?.core.plugins.stopFixtures(); });
  return <View class="flex-1">
    <If when={!!ready()}><AppShell initialMode="light" initialOverlay="settings"
      initialSettingsCategory={props.category} initialPluginLogin={props.login ? "subsonic.music" : undefined}
      initialPluginResult={props.category === "开发者" ? ready()!.result : undefined} /></If>
    <If when={!!ready.error()}><Text>{String(ready.error())}</Text></If>
  </View>;
}
