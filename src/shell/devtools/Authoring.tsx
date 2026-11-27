import { effect, Image, Scroll, signal, Text, token, View } from "nsh";
import { Button as BaseButton, Input } from "nsh/base";
import type { Core } from "haiku/native";
import { Button, focus } from "../../ui/Button";
import { Field } from "../../ui/LibraryControls";
import { ActionMenu } from "../../ui/Surfaces";
import type { PluginWorkspace, WorkspaceStatus } from "../../state/devtools";
import { CodeView, Dot, Pane, Pill, Placeholder, type Tone } from "./Kit";

const phases: Record<WorkspaceStatus["phase"], { label: string; tone: Tone; step: number }> = {
  waiting: { label: "等待开始", tone: "muted", step: 1 },
  writing: { label: "编写中", tone: "primary", step: 1 },
  loaded: { label: "已热加载", tone: "success", step: 2 },
  failed: { label: "加载失败", tone: "destructive", step: 2 },
  done: { label: "已完成", tone: "success", step: 3 },
};
const steps = ["创建工作区", "Agent 编写", "热加载", "完成"];

function Logo(props: { src?: string | null; size: "sm" | "lg" }) {
  const size = props.size === "lg" ? "size-12 rounded-(--radius-panel)" : "size-7 rounded-(--radius-inset)";
  return <View class={`shrink-0 overflow-hidden bg-muted ${size}`}>
    <If when={!!props.src}><Image src={props.src!} class="size-full" style={{ objectFit: "cover" }} /></If>
  </View>;
}

/** Create an agent workspace, hand its prompt over, and follow the STATUS file the agent writes. */
export function AuthoringPanel(props: { core: Core }) {
  const workspaces = signal<PluginWorkspace[]>([]), selected = signal(""), creating = signal(false);
  const status = signal<WorkspaceStatus | null>(null), error = signal(""), busy = signal(false);
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
      if (next.phase !== status()?.phase && (next.phase === "done" || next.phase === "loaded")) void reload();
      status.set(next);
    }).then((s) => { if (alive) stop = s; else s(); })
      .catch((e) => status.set({ phase: "failed", message: String(e) }));
    return () => { alive = false; stop?.(); };
  });
  const showCreate = () => creating() || !workspaces().length;
  return <View class="flex-1 min-h-0 flex-row gap-3" testID="plugin-authoring">
    <Pane class="w-64 shrink-0" title="工作区"
      accessory={<Button label="新建" size="sm" variant={showCreate() ? "ghost" : "secondary"} selected={showCreate()}
        onPress={() => creating.set(true)} />}>
      <If when={!workspaces().length}>
        <View class="flex-1 items-center justify-center p-4">
          <Text class="text-center text-xs text-muted-foreground">还没有工作区。新建后会出现在这里。</Text>
        </View>
      </If>
      <Scroll class="flex-1 min-h-0" contentClass="gap-0.5 p-1.5">
        {workspaces().map((workspace) => <BaseButton selected={!showCreate() && selected() === workspace.path}
          onPress={() => { creating.set(false); selected.set(workspace.path); }}
          accessibilityLabel={`工作区 ${workspace.name}`}
          class={`group flex-row items-center gap-2.5 rounded-(--radius-inset) px-2 py-1.5 active:bg-muted selected:bg-selection ${focus}`}>
          <Logo src={workspace.logo} size="sm" />
          <View class="flex-1 min-w-0 gap-0.5">
            <Text class="text-foreground group-selected:font-medium group-selected:text-selection-foreground"
              style={{ fontSize: token("sidebar-text-size"), numberOfLines: 1 }}>{workspace.name}</Text>
            <Text class="font-mono text-muted-foreground" style={{ fontSize: token("sidebar-hint-size"), numberOfLines: 1 }}>{workspace.pluginId}</Text>
          </View>
          <Dot tone={workspace.done ? "success" : "primary"} />
        </BaseButton>)}
      </Scroll>
    </Pane>
    <Pane class="flex-1">
      <If when={showCreate()}>
        <CreateWorkspace core={props.core} busy={busy()} error={error()} onCancel={workspaces().length ? () => creating.set(false) : undefined}
          onCreate={(name, author, website, description) => act(async () => {
            const workspace = await props.core.plugins.createWorkspace(name, author, website, description) as PluginWorkspace;
            await reload();
            selected.set(workspace.path); creating.set(false);
            await props.core.plugins.copyWorkspacePrompt(workspace.path);
          })} />
      </If>
      <If when={!showCreate() && !!current()}>
        <WorkspaceDetail core={props.core} workspace={current()!} status={status()} error={error()}
          act={act} onRemoved={async () => { selected.set(""); await reload(); if (workspaces()[0]) selected.set(workspaces()[0].path); }} />
      </If>
    </Pane>
  </View>;
}

function CreateWorkspace(props: {
  core: Core; busy: boolean; error: string; onCancel?: () => void;
  onCreate: (name: string, author: string, website: string, description: string) => Promise<void>;
}) {
  const name = signal(""), author = signal(""), website = signal(""), description = signal("");
  return <Scroll class="flex-1 min-h-0" contentClass="items-center px-8 py-8">
    <View class="w-full max-w-120 gap-5">
      <Placeholder art="authoring" compact title="让 Agent 为你编写插件"
        description="填写插件信息后，Haiku 会创建工作区并复制一份提示词。把它粘贴给 Claude Code、Codex 等 Agent；Agent 写入状态文件时，这里会实时热加载并显示进度。" />
      <View class="gap-3 rounded-(--radius-panel) bg-muted/50 p-4">
        <View class="flex-row gap-3">
          <View class="flex-1"><Field label="插件名称" value={name()} placeholder="例如：我的音乐库" onChange={(v) => name.set(v)} /></View>
          <View class="flex-1"><Field label="作者" value={author()} placeholder="你的名字或团队" onChange={(v) => author.set(v)} /></View>
        </View>
        <View class="gap-1">
          <Field label="官网" value={website()} placeholder="https://… 用于获取官方图标和参考接口" onChange={(v) => website.set(v)} />
          <Text class="text-muted-foreground" style={{ fontSize: token("sidebar-hint-size") }}>Agent 会从官网获取清晰的官方图标；留空则由 Agent 自行查找。</Text>
        </View>
        <View class="gap-1">
          <Text class="text-xs text-muted-foreground">需求描述</Text>
          <Input accessibilityLabel="需求描述" multiline value={description()}
            placeholder="要接入的服务、需要的能力（搜索、播放、歌词、歌单…）、登录方式、可参考的接口文档等"
            onChange={(event) => description.set(event.value)} style={{ fontSize: token("control-text-size") }}
            class={`h-28 rounded-(--radius-inset) border border-input bg-card px-2 py-1 text-foreground placeholder:text-muted-foreground ${focus}`} />
        </View>
        <If when={!!props.error}><Text class="text-xs text-destructive">{props.error}</Text></If>
        <View class="flex-row items-center justify-end gap-2">
          <If when={!!props.onCancel}><Button label="取消" onPress={() => props.onCancel?.()} /></If>
          <Button label="创建并复制提示词" variant="primary" disabled={!name().trim()} loading={props.busy}
            onPress={() => void props.onCreate(name(), author(), website(), description())} />
        </View>
      </View>
    </View>
  </Scroll>;
}

function WorkspaceDetail(props: {
  core: Core; workspace: PluginWorkspace; status: WorkspaceStatus | null; error: string;
  act: (work: () => Promise<unknown>) => Promise<void>; onRemoved: () => Promise<void>;
}) {
  const copied = signal(false);
  const w = () => props.workspace;
  const phase = () => props.status ? phases[props.status.phase] : undefined;
  const plugin = () => props.status?.plugin;
  return <Scroll class="flex-1 min-h-0" contentClass="gap-5 p-5" testID="workspace-status">
    <View class="flex-row items-center gap-3">
      <Logo src={w().logo} size="lg" />
      <View class="flex-1 min-w-0 gap-0.5">
        <View class="flex-row items-center gap-2">
          <Text role="header" class="text-lg font-semibold text-foreground" style={{ numberOfLines: 1 }}>{w().name}</Text>
          <If when={!!phase()}><Pill tone={phase()!.tone} label={phase()!.label} /></If>
        </View>
        <Text class="font-mono text-xs text-muted-foreground" style={{ numberOfLines: 1 }}>
          {[w().pluginId, plugin()?.version, w().author].filter(Boolean).join(" · ")}
        </Text>
      </View>
      <Button label={copied() ? "已复制" : "复制提示词"} variant="primary"
        onPress={() => void props.act(async () => {
          await props.core.plugins.copyWorkspacePrompt(w().path); copied.set(true); setTimeout(() => copied.set(false), 1600);
        })} />
      <ActionMenu label="…" accessibilityLabel="更多工作区操作" items={[
        { label: "在访达中显示", onSelect: () => void props.act(() => props.core.plugins.revealWorkspace(w().path)) },
        { label: "删除工作区", separatorBefore: true, onSelect: () => void props.act(async () => {
          await props.core.plugins.removeWorkspace(w().path); await props.onRemoved();
        }) },
      ]} />
    </View>
    <Stepper step={phase()?.step ?? 1} failed={props.status?.phase === "failed"} />
    <If when={!!props.error}><Text class="text-xs text-destructive">{props.error}</Text></If>
    <Pane title="当前状态">
      <View class="gap-1 px-3 py-3">
        <Text class={`text-sm ${props.status?.phase === "failed" ? "text-destructive" : "text-foreground"}`}>
          {props.status?.phase === "failed" ? "最近一次加载失败，Agent 会从 LOAD 文件读到同样的原因：" : props.status?.message ?? "正在读取状态…"}
        </Text>
        <If when={props.status?.phase === "waiting"}>
          <Text class="text-xs text-muted-foreground">复制提示词并粘贴给 Agent。Agent 开始后会写入 STATUS 文件，这里随之更新，无需手动刷新。</Text>
        </If>
      </View>
      <If when={props.status?.phase === "failed"}>
        <View class="h-40 border-t border-border bg-destructive/5"><CodeView text={props.status!.message} /></View>
      </If>
    </Pane>
    <If when={!!plugin()}>
      <Pane title="插件能力" accessory={<Text class="text-xs text-muted-foreground">{plugin()!.kind}</Text>}>
        <View class="flex-row flex-wrap gap-1.5 p-3">
          {plugin()!.capabilities.map((c) => <View class="rounded-(--radius-inset) bg-muted px-2 py-0.5">
            <Text class="font-mono text-xs text-foreground">{c}</Text>
          </View>)}
          <If when={!plugin()!.capabilities.length}><Text class="text-xs text-muted-foreground">尚未实现内容能力。</Text></If>
        </View>
      </Pane>
    </If>
    <If when={!!props.status?.summary}>
      <Pane title="完成总结">
        <View selectable class="p-3"><Text class="text-sm text-foreground">{props.status!.summary}</Text></View>
      </Pane>
    </If>
    <If when={props.status?.phase === "done"}>
      <Text class="text-xs text-muted-foreground">插件已安装：在 设置 → 插件 中添加来源即可使用，或切换到“插件调试”调用它的能力。</Text>
    </If>
    <View class="gap-1">
      <Text class="font-medium text-muted-foreground" style={{ fontSize: token("sidebar-hint-size") }}>工作区位置</Text>
      <View selectable><Text class="font-mono text-xs text-muted-foreground">{w().path}</Text></View>
    </View>
  </Scroll>;
}

/** Four fixed stages; the current one is emphasized, a failed load marks the third. */
function Stepper(props: { step: number; failed: boolean }) {
  return <View class="flex-row items-center gap-2">
    {steps.map((label, index) => {
      const done = index < props.step || (index === props.step && index === steps.length - 1);
      const active = index === props.step && !done;
      const failed = active && props.failed;
      return <>
        <If when={index > 0}><View class={`h-px flex-1 ${index <= props.step ? "bg-primary" : "bg-border"}`} /></If>
        <View class="flex-row items-center gap-1.5">
          <View class={`size-5 items-center justify-center rounded-full ${failed ? "bg-destructive" : done ? "bg-primary" : active ? "border-2 border-primary" : "border border-border"}`}>
            <Text class={`text-xs font-semibold ${failed ? "text-destructive-foreground" : done ? "text-primary-foreground" : active ? "text-primary" : "text-muted-foreground"}`}
              style={{ fontVariant: ["tabular-nums"] }}>{failed ? "!" : done ? "✓" : String(index + 1)}</Text>
          </View>
          <Text class={`text-xs ${active || done ? "font-medium text-foreground" : "text-muted-foreground"}`}>{label}</Text>
        </View>
      </>;
    })}
  </View>;
}
