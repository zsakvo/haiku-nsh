import { cleanup, effect, signal, Text, token, View } from "nsh";
import { Tab, Tabs } from "nsh/base";
import { titleBar, Window } from "nsh/desktop";
import { focus } from "../ui/Button";
import { chromeHeight } from "../ui/Chrome";
import { createPlugins } from "../state/plugins";
import { startHttpCapture, stopHttpCapture } from "../state/devtools";
import { LogsView } from "./Diagnostics";
import { AuthoringPanel } from "./devtools/Authoring";
import { DebuggerPanel } from "./devtools/Debugger";
import { NetworkPanel } from "./devtools/Network";

const WINDOW_ID = "devtools";
const sections = ["插件编写", "插件调试", "网络请求", "日志查看"] as const;
type Section = typeof sections[number];

/** Open the developer tools window, or bring the open one to the front. */
export function openDevTools() {
  const existing = Window.get(WINDOW_ID);
  if (existing) { existing.focus(); return; }
  Window.open(() => <DevTools />, {
    id: WINDOW_ID, title: "开发工具", defaultSize: { width: 1180, height: 780 }, minSize: { width: 960, height: 620 },
    defaultPosition: "center", restoreBounds: true, titleBar: { style: "hidden", height: token("toolbar-height") },
  });
}
export function closeDevTools() { Window.get(WINDOW_ID)?.close(); }

/** No sidebar: the title bar carries a centered segmented tab list; each tab owns the full body.
 * Plugin HTTP traffic is recorded only while this window exists. */
function DevTools() {
  const selected = signal<Section>(sections[0]);
  const plugins = createPlugins();
  effect(() => { const c = plugins.core(); if (c) startHttpCapture(c); });
  cleanup(stopHttpCapture);
  const logSources = () => plugins.value().sources.map((s) => ({ value: s.sourceId, title: s.displayName || s.sourceId }));
  return <View class="flex-1 flex-col bg-background">
    <View class="shrink-0 flex-row items-center" style={{ height: chromeHeight() }} desktop={{ windowDrag: true }}>
      <View class="shrink-0" style={{ width: titleBar().left }} />
      <View class="flex-1 flex-row justify-center">
        <Tabs testID="devtools-tabs" class="flex-row gap-0.5 rounded-(--radius) bg-muted p-0.5"
          indicatorClass="inset-y-0.5 rounded-(--radius-inset) bg-card shadow-sm">
          {sections.map((section) => <Tab selected={selected() === section} onPress={() => selected.set(section)}
            class={`group min-h-6 justify-center rounded-(--radius-inset) px-3.5 active:opacity-70 ${focus}`}>
            <Text class="text-muted-foreground group-selected:font-medium group-selected:text-foreground"
              style={{ fontSize: token("control-text-size") }}>{section}</Text>
          </Tab>)}
        </Tabs>
      </View>
      <View class="shrink-0" style={{ width: titleBar().left }} />
    </View>
    <View class="flex-1 min-h-0 flex-col px-5 pt-2 pb-5">
      <If when={!!plugins.core()}>
        <If when={selected() === "插件编写"}><AuthoringPanel core={plugins.core()!} /></If>
        <If when={selected() === "插件调试"}><DebuggerPanel state={plugins} /></If>
        <If when={selected() === "网络请求"}><NetworkPanel /></If>
        <If when={selected() === "日志查看"}><LogsView core={plugins.core()!} sources={logSources()} fill /></If>
      </If>
      <If when={!!plugins.core.error()}><Text class="text-sm text-destructive">{String(plugins.core.error())}</Text></If>
    </View>
  </View>;
}
