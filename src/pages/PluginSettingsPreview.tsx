import { cleanup, resource, Text, View } from "nsh";
import { Core } from "haiku/native";
import { AppShell } from "../shell/AppShell";

/** Real-window evidence uses the loopback HTTP fixture and an actual capability invocation. */
export function PluginSettingsPreview(props: { category: string; login?: boolean; loginPlugin?: string; devtools?: boolean }) {
  const ready = resource(async () => {
    const c = await Core.open();
    await c.settings.update({ themeMode: "light" });
    if (props.loginPlugin || props.devtools) {
      await c.plugins.importPackage("native/app/tests/fixtures/auth-plugin");
    }
    if (props.devtools) {
      const id = await c.sources.beginLogin("auth.fixture");
      await c.sources.loginStep(id, "cookie", "fake-preview-only");
    }
    await c.plugins.importPackage("dev-plugins/qq-music");
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
    const result = await c.plugins.invoke(props.devtools ? "auth.fixture" : source.sourceId,
      "track.get", [props.devtools ? "fixture" : "song:1/字"], {});
    if (props.devtools) {
      // Populate the real subscriptions after the settings surface has mounted.
      setTimeout(() => {
        void c.plugins.invoke("auth.fixture", "track.get", ["fixture"], {});
        void c.plugins.invoke(source.sourceId, "track.get", ["song:1/字"], {});
      }, 700);
    }
    console.log("M2-05 preview ready", props.category);
    return { core: c, result };
  });
  cleanup(() => { void ready()?.core.plugins.stopFixtures(); });
  return <View class="flex-1">
    <If when={!!ready()}><AppShell initialMode="light" initialOverlay="settings"
      initialSettingsCategory={props.category} initialPluginLogin={props.login ? props.loginPlugin || "subsonic.music" : undefined}
      initialPluginResult={props.category === "开发者" ? ready()!.result : undefined} /></If>
    <If when={!!ready.error()}><Text>{String(ready.error())}</Text></If>
  </View>;
}
