import { cleanup, effect, Scroll, signal, Text, token, View } from "nsh";
import { Button as BaseButton, Tab, Tabs } from "nsh/base";
import { Button, focus } from "../../ui/Button";
import { Field, ToggleGroup } from "../../ui/LibraryControls";
import { SettingSelect } from "../../ui/SettingsControls";
import { ActionMenu } from "../../ui/Surfaces";
import type { PluginsState } from "../../state/plugins";
import { CodeView, copyText, Dot, Pane, Placeholder } from "./Kit";

interface Outcome { label: string; value: unknown; elapsedMs: number; failed: boolean }
const views = ["树形", "JSON"] as const;
const modes = [{ id: "form", label: "表单" }, { id: "raw", label: "JSON" }];

/** Invoke installed capabilities and inspect what they return; optionally hot load a local package. */
export function DebuggerPanel(props: { state: PluginsState }) {
  const source = signal(""), capability = signal("track.get"), args = signal("[]"), mode = signal("form");
  const values = signal<Record<string, string>>({});
  const location = signal(""), watching = signal(""), watchStatus = signal("");
  const outcome = signal<Outcome | null>(null), view = signal<typeof views[number]>("树形");
  let stopWatch: (() => void) | undefined;
  cleanup(() => stopWatch?.());
  const sources = () => props.state.value().sources;
  const activeSource = () => source() || sources()[0]?.sourceId || "";
  const plugin = () => sources().find((s) => s.sourceId === activeSource())?.pluginId;
  const descriptor = () => props.state.value().packages.find((p) => p.id === plugin());
  const fields = () => descriptor()?.parameters?.[capability()] || [];
  effect(() => {
    const caps = descriptor()?.capabilities ?? [];
    if (caps.length && !caps.includes(capability())) capability.set(caps[0]);
  });
  effect(() => {
    const parameters = descriptor()?.parameters?.[capability()] || [];
    values.set(Object.fromEntries(parameters.map((field) => [field.key, field.default])));
  });
  function argumentsForCall() {
    if (mode() === "raw") return { args: JSON.parse(args()), context: { limit: 20 } };
    const argsValue: unknown[] = [], context: Record<string, unknown> = {};
    for (const field of fields()) {
      const input = values()[field.key] ?? field.default;
      if (input === "" && field.key === "cursor") continue;
      const value = field.type === "number" ? Number(input) : field.type === "json" ? JSON.parse(input) : input;
      if (field.target === "args") return { args: value as unknown[], context };
      if (field.target === "context") context[field.key] = value;
      else if (field.target === "arg") argsValue[field.index || 0] = value;
      else {
        const index = field.index || 0;
        argsValue[index] = { ...(argsValue[index] as object || {}), [field.key]: value };
      }
    }
    return { args: argsValue, context };
  }
  /** Every action lands in the result pane, failures included, with how long it took. */
  async function run(label: string, work: (c: NonNullable<ReturnType<PluginsState["core"]>>) => Promise<unknown>) {
    const started = Date.now();
    const value = await props.state.run(work);
    const failed = !!props.state.error();
    outcome.set({ label, elapsedMs: Date.now() - started, failed, value: failed ? props.state.error() : value ?? "完成" });
  }
  async function load() {
    stopWatch?.(); watching.set("");
    await props.state.run(async (c) => {
      await c.plugins.importPackage(location());
      stopWatch = await c.plugins.watchDirectory(location(), (status) => watchStatus.set(status));
      watching.set(location()); watchStatus.set("已加载，正在监听文件变化");
    });
  }
  const sourceTitle = () => sources().find((s) => s.sourceId === activeSource())?.displayName || activeSource();
  return <View class="flex-1 min-h-0 flex-row gap-3" testID="plugin-debugger">
    <View class="w-96 shrink-0 min-h-0 flex-col gap-3">
      <Pane class="flex-1" title="能力调用">
        <If when={!sources().length}>
          <Placeholder art="debug" compact title="还没有可调用的实例"
            description="在 设置 → 插件 中添加一个来源，或在下方加载本地插件目录后再来调用。" />
        </If>
        <If when={!!sources().length}>
          <Scroll class="flex-1 min-h-0" contentClass="gap-3 p-3">
            <SettingSelect label="实例" value={activeSource()} onChange={(v) => source.set(v)}
              items={sources().map((s) => ({ value: s.sourceId, title: s.displayName || s.sourceId }))} />
            <SettingSelect label="能力" value={capability()} onChange={(v) => capability.set(v)}
              items={(descriptor()?.capabilities ?? []).map((value) => ({ value, title: value }))} />
            <View class="h-px bg-border" />
            <View class="flex-row items-center justify-between">
              <Text class="text-xs font-medium text-muted-foreground">参数</Text>
              <ToggleGroup value={mode()} items={modes} onChange={(v) => mode.set(v)} />
            </View>
            <If when={mode() === "raw"}>
              <Field label="参数（JSON 数组）" value={args()} onChange={(v) => args.set(v)} />
            </If>
            <If when={mode() === "form"}><View class="gap-3" testID="plugin-parameter-form">
              <Each of={fields()} key={(field) => field.key}>{(field) => <Field label={field().label}
                value={values()[field().key] ?? field().default}
                onChange={(v) => values.set({ ...values(), [field().key]: v })} />}</Each>
              <If when={!fields().length}><Text class="text-xs text-muted-foreground">此能力无需参数。</Text></If>
            </View></If>
          </Scroll>
          <View class="shrink-0 flex-row items-center gap-2 border-t border-border px-3 py-2.5">
            <Button label="调用" variant="primary" disabled={!activeSource()} loading={props.state.busy()}
              onPress={() => void run(`${capability()} · ${sourceTitle()}`, async (c) => {
                const input = argumentsForCall();
                return c.plugins.invoke(activeSource(), capability(), input.args, input.context);
              })} />
            <Button label="契约测试" disabled={!plugin()} loading={props.state.busy()}
              onPress={() => void run(`契约测试 · ${plugin()}`, (c) => c.plugins.testContract(plugin()!))} />
            <View class="flex-1" />
            <ActionMenu label="…" accessibilityLabel="更多调试操作" items={[
              { label: "查看存储", disabled: !activeSource(), onSelect: () => void run(`存储 · ${sourceTitle()}`, (c) => c.plugins.storage(activeSource())) },
              { label: "清除 KV", separatorBefore: true, disabled: !activeSource(), onSelect: () => void run(`清除 KV · ${sourceTitle()}`, (c) => c.plugins.clearStorage(activeSource())) },
              { label: "清除凭据", disabled: !activeSource(), onSelect: () => void run(`清除凭据 · ${sourceTitle()}`, (c) => c.sources.logout(activeSource())) },
            ]} />
          </View>
        </If>
      </Pane>
      <Pane title="本地插件目录">
        <View class="gap-2.5 p-3">
          <View class="flex-row items-end gap-2">
            <View class="flex-1"><Field label="目录路径" value={location()} placeholder="包含 plugin.ts 的文件夹" onChange={(v) => location.set(v)} /></View>
            <Button label={watching() ? "重新加载" : "加载"} disabled={!location().trim()} loading={props.state.busy()} onPress={() => void load()} />
          </View>
          <If when={!!watching()}>
            <View class="flex-row items-center gap-2">
              <Dot tone={watchStatus().startsWith("重载失败") || watchStatus().startsWith("目录读取失败") ? "destructive" : "success"} />
              <Text class="flex-1 min-w-0 text-xs text-muted-foreground" style={{ numberOfLines: 2 }}>{watchStatus()}</Text>
              <Button label="停止" size="sm" variant="ghost" onPress={() => { stopWatch?.(); watching.set(""); watchStatus.set(""); }} />
            </View>
          </If>
        </View>
      </Pane>
    </View>
    <Pane class="flex-1" testID="plugin-call-result" title={outcome()?.label ?? "返回结果"}
      accessory={<If when={!!outcome()}>
        <View class="flex-row items-center gap-2">
          <Dot tone={outcome()!.failed ? "destructive" : "success"} />
          <Text class="text-xs text-muted-foreground" style={{ fontVariant: ["tabular-nums"] }}>{`${outcome()!.failed ? "失败" : "成功"} · ${outcome()!.elapsedMs} ms`}</Text>
          <Button label="复制" size="sm" variant="ghost" onPress={() => void copyText(stringify(outcome()!.value))} />
        </View>
      </If>}>
      <If when={!outcome()}>
        <Placeholder art="debug" title="调用一次能力看看"
          description="选择实例和能力、填好参数后点击“调用”。返回值、契约测试报告和存储内容都会显示在这里，失败时显示错误原因。" />
      </If>
      <If when={!!outcome()}>
        <If when={outcome()!.failed}>
          <View class="flex-1 bg-destructive/5"><CodeView text={String(outcome()!.value)} /></View>
        </If>
        <If when={!outcome()!.failed}>
          <View class="shrink-0 flex-row border-b border-border px-1.5">
            <Tabs class="flex-row" indicatorClass="bottom-0 h-0.5 mx-2 rounded-full bg-primary">
              {views.map((name) => <Tab selected={view() === name} onPress={() => view.set(name)} class={`group px-2 py-2 ${focus}`}>
                <Text class="text-xs text-muted-foreground group-selected:font-medium group-selected:text-foreground">{name}</Text>
              </Tab>)}
            </Tabs>
          </View>
          <If when={view() === "树形"}>
            <Scroll class="flex-1 min-h-0" contentClass="items-start p-3" testID="plugin-result-scroll">
              <View selectable><JsonTree value={outcome()!.value} depth={0} /></View>
            </Scroll>
          </If>
          <If when={view() === "JSON"}><CodeView text={stringify(outcome()!.value)} /></If>
        </If>
      </If>
    </Pane>
  </View>;
}

function stringify(value: unknown) {
  return typeof value === "string" ? value : JSON.stringify(value, null, 2) ?? String(value);
}
function scalar(value: unknown) {
  if (typeof value === "string") return { text: JSON.stringify(value), class: "text-success" };
  if (typeof value === "number" || typeof value === "boolean") return { text: String(value), class: "text-primary" };
  return { text: value === null ? "null" : JSON.stringify(value) ?? "undefined", class: "text-muted-foreground" };
}

/** Collapsible data inspector: two levels open, deeper branches folded with their size. */
export function JsonTree(props: { value: unknown; name?: string; depth: number }) {
  const expanded = signal(props.depth < 2);
  const array = () => Array.isArray(props.value);
  const entries = () => props.value && typeof props.value === "object" && Object.keys(props.value).length
    ? Object.entries(props.value) : null;
  const size = () => `${array() ? "[" : "{"}${entries()?.length ?? 0}${array() ? "]" : "}"}`;
  return <View class="items-start">
    <If when={!!entries()}>
      <BaseButton accessibilityLabel={`${expanded() ? "收起" : "展开"} ${props.name ?? "结果"}`}
        class={`min-h-6 flex-row items-center gap-1 rounded-(--radius-inset) pe-1 active:bg-muted ${focus}`}
        onPress={() => expanded.set(!expanded())}>
        <Text class="w-3 text-xs text-muted-foreground">{expanded() ? "▾" : "▸"}</Text>
        <Text class="font-mono text-xs text-foreground">{props.name ?? (array() ? "数组" : "对象")}</Text>
        <Text class="font-mono text-xs text-muted-foreground">{size()}</Text>
      </BaseButton>
      <If when={expanded()}><View class="ms-1.5 border-s border-border ps-2.5 items-start">
        <Each of={entries() || []} key={(e) => e[0]}>{(entry) =>
          <JsonTree name={entry()[0]} value={entry()[1]} depth={props.depth + 1} />}</Each>
      </View></If>
    </If>
    <If when={!entries()}>
      <View class="min-h-6 flex-row items-start gap-1.5 ps-4">
        <If when={props.name !== undefined}><Text class="font-mono text-xs text-muted-foreground">{`${props.name}:`}</Text></If>
        <Text class={`font-mono text-xs ${scalar(props.value).class}`}>{scalar(props.value).text}</Text>
      </View>
    </If>
  </View>;
}
