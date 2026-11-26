import { Clipboard, effect, List, Scroll, signal, Text, token, View } from "nsh";
import { Input, Button as BaseButton } from "nsh/base";
import { Window } from "nsh/desktop";
import type { Core } from "haiku/native";
import { Button, focus } from "../ui/Button";
import { Field } from "../ui/LibraryControls";
import { SidebarItem } from "../ui/Navigation";
import { SearchField } from "../ui/SearchField";
import { ShellFrame, ShellSidebar, SidebarChrome, WindowPanel, chromeHeight } from "../ui/Chrome";
import { SettingCardBlock, SettingRow, SettingSection } from "../ui/SettingsControls";
import type { IconName } from "../ui/Icon";
import { createPlugins, type PluginsState } from "../state/plugins";
import { clearTraces, httpTraces, startTraceCapture, type HttpTrace, type PluginWorkspace, type WorkspaceStatus } from "../state/devtools";
import { PluginDebugger } from "./PluginSettings";
import { LogsView } from "./Diagnostics";

const WINDOW_ID = "devtools";
const sections = [
  { label: "插件编写", icon: "edit" }, { label: "网络请求", icon: "radio" }, { label: "日志查看", icon: "playlist" },
] as const satisfies readonly { label: string; icon: IconName }[];

/** Open the developer tools window, or bring the open one to the front. */
export function openDevTools() {
  const existing = Window.get(WINDOW_ID);
  if (existing) { existing.focus(); return; }
  Window.open(() => <DevTools />, {
    id: WINDOW_ID, title: "开发工具", defaultSize: { width: 1120, height: 760 }, minSize: { width: 900, height: 600 },
    defaultPosition: "center", restoreBounds: true, titleBar: { style: "hidden", height: token("toolbar-height") },
  });
}
export function closeDevTools() { Window.get(WINDOW_ID)?.close(); }

function DevTools() {
  const selected = signal<string>(sections[0].label);
  const plugins = createPlugins();
  effect(() => { const c = plugins.core(); if (c) startTraceCapture(c); });
  return <ShellFrame>
    <ShellSidebar class="border-r border-sidebar-border">
      <SidebarChrome />
      <View class="flex-col gap-(--navigation-row-gap) px-2">
        {sections.map((section) => <SidebarItem icon={section.icon} label={section.label}
          selected={selected() === section.label} onPress={() => selected.set(section.label)} />)}
      </View>
    </ShellSidebar>
    <WindowPanel>
      <View class="shrink-0 flex-row items-center px-6" style={{ height: chromeHeight() }} desktop={{ windowDrag: true }}>
        <Text role="header" class="text-sm font-medium text-foreground">{selected()}</Text>
      </View>
      <If when={!!plugins.core()}>
        <If when={selected() === "插件编写"}>
          <Scroll class="flex-1" contentClass="flex-col gap-5 px-6 pb-8">
            <PluginAuthoring core={plugins.core()!} plugins={plugins} />
            <View class="flex-col gap-2">
              <Text role="header" class="text-sm font-medium text-foreground">调试已安装插件</Text>
              <PluginDebugger state={plugins} />
            </View>
            <If when={!!plugins.error()}><Text class="text-sm text-destructive">{plugins.error()}</Text></If>
          </Scroll>
        </If>
        <If when={selected() === "网络请求"}><View class="flex-1 min-h-0 flex-col px-6 pb-6"><NetworkRequests /></View></If>
        <If when={selected() === "日志查看"}><View class="flex-1 min-h-0 flex-col px-6 pb-6"><LogsView core={plugins.core()!} fill /></View></If>
      </If>
      <If when={!!plugins.core.error()}><Text class="px-6 text-sm text-destructive">{String(plugins.core.error())}</Text></If>
    </WindowPanel>
  </ShellFrame>;
}

const phaseLabels: Record<WorkspaceStatus["phase"], string> = {
  waiting: "等待编写", failed: "加载失败", loaded: "编写中（已热加载）", done: "已完成",
};

/** Create an agent workspace, hand its prompt over, and follow the agent until DONE. */
function PluginAuthoring(props: { core: Core; plugins: PluginsState }) {
  const name = signal(""), author = signal(""), description = signal("");
  const workspaces = signal<PluginWorkspace[]>([]), selected = signal("");
  const status = signal<WorkspaceStatus | null>(null), notice = signal(""), error = signal(""), busy = signal(false);
  const current = () => workspaces().find((w) => w.path === selected());
  async function act(work: () => Promise<unknown>) {
    busy.set(true); error.set("");
    try { await work(); } catch (e) { error.set(String(e)); } finally { busy.set(false); }
  }
  async function reload() { workspaces.set(await props.core.plugins.workspaces() as PluginWorkspace[]); }
  void act(async () => { await reload(); if (!selected() && workspaces()[0]) selected.set(workspaces()[0].path); });
  effect(() => {
    const path = selected();
    status.set(null);
    if (!path) return;
    let alive = true, stop: (() => void) | undefined;
    props.core.plugins.watchWorkspace(path, (value) => {
      const next = value as WorkspaceStatus;
      if (next.phase === "done" && status()?.phase !== "done") void reload();
      status.set(next);
    }).then((s) => { if (alive) stop = s; else s(); })
      .catch((e) => status.set({ phase: "failed", message: String(e) }));
    return () => { alive = false; stop?.(); };
  });
  async function copyPrompt(workspace: PluginWorkspace) {
    await Clipboard.writeText(workspace.prompt);
    notice.set("提示词已复制。打开你的 Agent（如 Claude Code、Codex）粘贴即可开始；Agent 写完后，这里会自动显示结果。");
  }
  const create = () => act(async () => {
    const workspace = await props.core.plugins.createWorkspace(name(), author(), description()) as PluginWorkspace;
    await reload();
    selected.set(workspace.path);
    await copyPrompt(workspace);
    name.set(""); description.set("");
  });
  return <View class="flex-row items-start gap-5 min-w-0" testID="plugin-authoring">
    <View class="flex-1 min-w-0 flex-col gap-5">
      <SettingSection title="新建插件" description="Haiku 会为插件创建工作区，并生成一组面向 Agent 的提示词；Agent 在工作区中编写代码，开发工具实时加载并显示结果。">
        <SettingCardBlock>
          <Field label="插件名称" value={name()} placeholder="例如：我的音乐库" onChange={(value) => name.set(value)} />
          <Field label="作者" value={author()} placeholder="你的名字或团队" onChange={(value) => author.set(value)} />
          <View class="gap-1">
            <Text class="text-xs text-muted-foreground">大致描述</Text>
            <Input accessibilityLabel="大致描述" multiline value={description()}
              placeholder="要接入的服务、需要的能力（搜索、播放、歌词、歌单…）、登录方式、可参考的接口文档等"
              onChange={(event) => description.set(event.value)} style={{ fontSize: token("control-text-size") }}
              class={`h-24 rounded-(--radius-inset) border border-input bg-card px-2 py-1 text-foreground placeholder:text-muted-foreground ${focus}`} />
          </View>
          <View class="flex-row">
            <Button label="创建并复制提示词" variant="primary" size="sm" disabled={!name().trim()} loading={busy()} onPress={() => void create()} />
          </View>
          <If when={!!notice()}><Text class="text-xs text-muted-foreground">{notice()}</Text></If>
          <If when={!!error()}><Text class="text-xs text-destructive">{error()}</Text></If>
        </SettingCardBlock>
      </SettingSection>
      <SettingSection title="工作区">
        <If when={!workspaces().length}><SettingCardBlock><Text class="text-xs text-muted-foreground">还没有插件工作区。</Text></SettingCardBlock></If>
        <View class="flex-col gap-0.5 p-1">
          {workspaces().map((workspace) => <BaseButton selected={selected() === workspace.path} onPress={() => selected.set(workspace.path)}
            accessibilityLabel={`工作区 ${workspace.name}`}
            class={`group flex-row items-center gap-3 rounded-(--radius-inset) px-2 py-1.5 active:bg-muted selected:bg-selection ${focus}`}>
            <View class="flex-1 min-w-0 gap-0.5">
              <Text class="text-foreground group-selected:font-medium group-selected:text-selection-foreground" style={{ fontSize: token("sidebar-text-size"), numberOfLines: 1 }}>{workspace.name}</Text>
              <Text class="text-muted-foreground" style={{ fontSize: token("sidebar-hint-size"), numberOfLines: 1 }}>{workspace.pluginId}</Text>
            </View>
            <Text class="text-muted-foreground" style={{ fontSize: token("sidebar-hint-size") }}>{workspace.done ? "已完成" : "进行中"}</Text>
          </BaseButton>)}
        </View>
      </SettingSection>
    </View>
    <View class="flex-1 min-w-0 flex-col gap-5">
      <If when={!!current()}>
        <SettingSection title={current()!.name} testID="workspace-status">
          <SettingRow title="状态" description={status()?.message ?? "正在读取…"}>
            <Text class={status()?.phase === "failed" ? "text-xs text-destructive" : "text-xs text-foreground"}>{status() ? phaseLabels[status()!.phase] : ""}</Text>
          </SettingRow>
          <If when={!!status()?.plugin}>
            <SettingRow divider title={`${status()!.plugin!.name} ${status()!.plugin!.version}`}
              description={`${status()!.plugin!.id} · ${status()!.plugin!.capabilities.join("、") || "无内容能力"}`} />
          </If>
          <If when={!!status()?.summary}>
            <SettingCardBlock divider><Text class="text-xs text-foreground">{status()!.summary}</Text></SettingCardBlock>
          </If>
          <If when={status()?.phase === "done"}>
            <SettingCardBlock divider><Text class="text-xs text-muted-foreground">插件已安装，可在 设置 → 插件 中添加来源，或在下方调试其能力。</Text></SettingCardBlock>
          </If>
          <SettingCardBlock divider>
            <Text class="text-xs text-muted-foreground" style={{ numberOfLines: 2 }}>{current()!.path}</Text>
            <View class="flex-row flex-wrap gap-2">
              <Button label="复制提示词" size="sm" onPress={() => void act(() => copyPrompt(current()!))} />
              <Button label="在访达中显示" size="sm" onPress={() => void act(() => props.core.plugins.revealWorkspace(current()!.path))} />
              <Button label="删除工作区" size="sm" variant="destructive" onPress={() => void act(async () => {
                const path = current()!.path;
                selected.set("");
                await props.core.plugins.removeWorkspace(path);
                await reload();
              })} />
            </View>
          </SettingCardBlock>
        </SettingSection>
      </If>
    </View>
  </View>;
}

function clock(time: number) {
  const d = new Date(time), pad = (v: number, n = 2) => String(v).padStart(n, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(d.getMilliseconds(), 3)}`;
}

/** Redacted plugin HTTP traffic heard since the app started with developer mode on. */
function NetworkRequests() {
  const query = signal(""), selected = signal<number | null>(null);
  const items = () => {
    const q = query().trim().toLowerCase();
    return httpTraces().filter((t) => !q || t.url.toLowerCase().includes(q) || t.sourceId.toLowerCase().includes(q)).slice().reverse();
  };
  const detail = () => httpTraces().find((t) => t.seq === selected());
  return <View class="flex-1 min-h-0 flex-col gap-3" testID="network-requests">
    <View class="flex-row items-center gap-2">
      <View class="flex-1"><SearchField label="筛选请求" value={query()} placeholder="按 URL 或来源筛选" onChange={(value) => query.set(value)} /></View>
      <Text class="text-xs text-muted-foreground">{`${items().length} 条`}</Text>
      <Button label="清空" size="sm" disabled={!httpTraces().length} onPress={() => { clearTraces(); selected.set(null); }} />
    </View>
    <View class="flex-1 min-h-0 flex-row gap-3">
      <List each={items()} keyExtractor={(t: HttpTrace) => String(t.seq)} estimatedItemSize={52}
        class="flex-1 min-h-0 rounded-(--radius) border border-border"
        empty={<Text class="p-3 text-xs text-muted-foreground">尚无请求。插件发起 HTTP 请求后显示在这里（请求头已脱敏）。</Text>}>
        {(t: () => HttpTrace) => <BaseButton selected={selected() === t().seq} onPress={() => selected.set(t().seq)}
          class={`gap-0.5 border-b border-border px-3 py-2 active:bg-muted selected:bg-selection ${focus}`}>
          <Text class="text-xs text-muted-foreground">{`${clock(t().time)} · ${t().method} · ${t().phase === "response" ? t().status ?? "响应" : "请求"} · ${t().sourceId}`}</Text>
          <Text class="text-xs text-foreground font-mono" style={{ numberOfLines: 1, textOverflow: "ellipsis" }}>{t().url}</Text>
        </BaseButton>}
      </List>
      <View class="w-80 min-h-0 flex-col rounded-(--radius) border border-border">
        <If when={!detail()}><Text class="p-3 text-xs text-muted-foreground">选择一条请求查看详情。</Text></If>
        <If when={!!detail()}>
          <Scroll class="flex-1" contentClass="gap-2 p-3">
            <Text class="text-sm font-medium text-foreground">{`${detail()!.method} ${detail()!.phase === "response" ? detail()!.status ?? "" : "请求"}`}</Text>
            <Text class="text-xs text-foreground font-mono">{detail()!.url}</Text>
            <Text class="text-xs text-muted-foreground">{`${clock(detail()!.time)} · 来源 ${detail()!.sourceId}`}</Text>
            <Text class="pt-2 text-xs font-medium text-muted-foreground">{detail()!.phase === "response" ? "响应头" : "请求头"}</Text>
            {Object.entries(detail()!.headers).map(([key, value]) =>
              <Text class="text-xs text-foreground font-mono">{`${key}: ${value}`}</Text>)}
            <If when={!Object.keys(detail()!.headers).length}><Text class="text-xs text-muted-foreground">无</Text></If>
          </Scroll>
        </If>
      </View>
    </View>
  </View>;
}
