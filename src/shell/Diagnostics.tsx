import { cleanup, List, resource, signal, Text, token, View } from "nsh";
import { Button as BaseButton } from "nsh/base";
import type { Core } from "haiku/native";
import { Button, focus } from "../ui/Button";
import { ToggleGroup } from "../ui/LibraryControls";
import { SettingRow, SettingSection, SettingSelect, SettingSwitchRow } from "../ui/SettingsControls";
import { ActionMenu } from "../ui/Surfaces";
import { clock, copyText, FilterField, Pane, Pill, Placeholder, prettyBody, type Tone } from "./devtools/Kit";
import type { createSettings } from "../state/settings";
import { Dialog, Menu, MenuItem } from "../ui/Surfaces";
import { localDateTime } from "../ui/dateTime";
import { AboutContent } from "./About";
import { BackupSettings } from "./BackupSettings";

const PAGE = 100;
const levels = [{ id: "", label: "全部" }, { id: "info", label: "信息" }, { id: "warn", label: "警告" }, { id: "error", label: "错误" }];
const levelPills: Record<string, { tone: Tone; label: string }> = {
  debug: { tone: "muted", label: "调试" }, info: { tone: "muted", label: "信息" },
  warn: { tone: "warning", label: "警告" }, error: { tone: "destructive", label: "错误" },
};
interface LogEntry { time: string; channel: string; level: string; source: string; session: number; message: string; detail?: string }
const entryKey = (e: LogEntry) => `${e.time}|${e.session}|${e.source}|${e.message.slice(0, 64)}`;

/** Newest first, loaded continuously as the list nears its end; filtering and redaction come
 * from Rust. Copy, export and clear are explicit actions in the overflow menu. */
export function LogsView(props: { core: Core; fill?: boolean; sources?: { value: string; title: string }[] }) {
  const service = props.core.logs;
  const revision = signal(0), channel = signal(""), level = signal(""), source = signal(""), query = signal("");
  const limit = signal(PAGE), expanded = signal<Set<string>>(new Set());
  const confirming = signal(false), error = signal("");
  cleanup(service.onChange(() => revision.update((v) => v + 1)));
  const filter = () => ({ channel: channel(), level: level(), source: source(), query: query() });
  const filtered = () => !!(channel() || level() || source() || query());
  const refilter = (apply: () => void) => { apply(); limit.set(PAGE); };
  const result = resource(
    () => ({ revision: revision(), filter: filter(), limit: limit() }),
    (input) => service.page(input.filter, 0, input.limit),
  );
  // Keep the rows on screen while the next window of rows loads.
  let last: { total: number; items: LogEntry[] } = { total: 0, items: [] };
  const data = () => { const next = result() as typeof last | undefined; if (next) last = next; return last; };
  const loading = () => result.loading();
  async function act(action: () => Promise<unknown>) {
    try { await action(); error.set(""); } catch (e) { error.set(String(e)); }
  }
  const toggle = (key: string) => expanded.update((previous) => {
    const next = new Set(previous); if (next.has(key)) next.delete(key); else next.add(key); return next;
  });
  return <View testID="logs-viewer" class={props.fill ? "flex-1 min-h-0 gap-3" : "gap-3"}>
    <If when={!props.fill}><Text role="header" class="text-lg font-semibold text-foreground">日志查看器</Text></If>
    <View class="shrink-0 flex-row items-center gap-3">
      <FilterField class="flex-1" label="日志关键词" value={query()} placeholder="搜索消息或详情" onChange={(v) => refilter(() => query.set(v))} />
      <ToggleGroup value={level()} items={levels} onChange={(v) => refilter(() => level.set(v))} />
      <SettingSelect inline label="日志频道" value={channel()} onChange={(v) => refilter(() => channel.set(v))}
        items={[{ value: "", title: "全部频道" }, { value: "app", title: "应用" }, { value: "plugin", title: "插件" }]} />
      <If when={!!props.sources?.length}>
        <SettingSelect inline label="日志来源" value={source()} onChange={(v) => refilter(() => source.set(v))}
          items={[{ value: "", title: "全部来源" }, ...(props.sources ?? [])]} />
      </If>
      <ActionMenu label="…" accessibilityLabel="更多日志操作" items={[
        { label: "复制筛选结果", onSelect: () => void act(async () => copyText(await service.copyText(filter()))) },
        { label: "导出全部日志…", onSelect: () => void act(() => service.exportLogs()) },
        { label: "清空日志…", separatorBefore: true, onSelect: () => confirming.set(true) },
      ]} />
    </View>
    <If when={!!error() || !!result.error()}><Text class="text-xs text-destructive">{error() || String(result.error())}</Text></If>
    <Pane class={props.fill ? "flex-1" : "h-96"}
      title={data().total ? `${filtered() ? "匹配" : "共"} ${data().total} 条 · 最新在上` : "日志"}
      accessory={<If when={loading()}><Text class="text-xs text-muted-foreground">正在加载…</Text></If>}>
      <If when={!data().items.length && !loading()}>
        <Placeholder art="logs" compact={!props.fill}
          title={filtered() ? "没有匹配的日志" : "暂无日志"}
          description={filtered() ? "换个关键词，或放宽级别、频道与来源筛选。" : "应用与插件运行时写下的日志会按时间倒序出现在这里。"}>
          <If when={filtered()}>
            <Button label="清除筛选" size="sm" onPress={() => refilter(() => { query.set(""); level.set(""); channel.set(""); source.set(""); })} />
          </If>
        </Placeholder>
      </If>
      <If when={!!data().items.length}>
        <List each={data().items} keyExtractor={entryKey} estimatedItemSize={44} class="flex-1 min-h-0"
          loadMoreThreshold={1.5}
          onLoadMore={() => { if (!loading() && data().items.length < data().total) limit.set(data().items.length + PAGE); }}>
          {(e: () => LogEntry) => <BaseButton onPress={() => toggle(entryKey(e()))} selected={expanded().has(entryKey(e()))}
            accessibilityLabel={`${e().level} ${e().message}`}
            class={`flex-row items-start gap-3 border-b border-border px-3 py-2 active:bg-muted selected:bg-muted ${focus}`}>
            <View class="w-30 shrink-0 gap-0.5">
              <Text class="font-mono text-xs text-foreground" style={{ fontVariant: ["tabular-nums"] }}>{clock(e().time)}</Text>
              <Text class="text-muted-foreground" style={{ fontSize: token("sidebar-hint-size"), fontVariant: ["tabular-nums"] }}>{localDateTime(e().time).slice(0, 10)}</Text>
            </View>
            <View class="w-10 shrink-0 items-start pt-px">
              <Pill tone={(levelPills[e().level] ?? levelPills.info).tone} label={(levelPills[e().level] ?? { label: e().level }).label} />
            </View>
            <View class="flex-1 min-w-0 gap-1">
              <Text class="text-muted-foreground" style={{ fontSize: token("sidebar-hint-size"), numberOfLines: 1 }}>
                {`${e().source || e().channel} · ${e().channel === "plugin" ? "插件" : "应用"} · 会话 ${e().session}`}
              </Text>
              <Text class="font-mono text-xs text-foreground" style={{ numberOfLines: expanded().has(entryKey(e())) ? 0 : 2, textOverflow: "ellipsis" }}>{e().message}</Text>
              <If when={expanded().has(entryKey(e())) && !!e().detail}>
                <View selectable class="mt-1 rounded-(--radius-inset) bg-muted px-2 py-1.5">
                  <Text class="font-mono text-xs text-muted-foreground">{prettyBody(e().detail)}</Text>
                </View>
              </If>
            </View>
          </BaseButton>}
        </List>
      </If>
    </Pane>
    <Dialog open={confirming()} onOpenChange={(e) => confirming.set(e.open)} class="w-96 p-5 gap-4">
      <Text class="text-lg font-semibold text-foreground">清空日志？</Text>
      <Text class="text-sm text-muted-foreground">{channel() ? `清空${channel() === "plugin" ? "插件" : "应用"}频道的全部日志，此操作无法撤销。` : "清空应用与插件两个频道的全部日志，此操作无法撤销。"}</Text>
      <View class="flex-row justify-end gap-2">
        <Button label="取消" onPress={() => confirming.set(false)} />
        <Button label="确认清空日志" variant="destructive" onPress={() => void act(async () => {
          await service.clearLogs(channel()); limit.set(PAGE); confirming.set(false);
        })} />
      </View>
    </Dialog>
  </View>;
}
/** Logs and software identification live in Settings → Other. */
export function OtherSettings(props: { core: Core; settings: ReturnType<typeof createSettings>; initialLogs?: boolean; backupPath?: string; initialBackupDialog?: "export" | "import" }) {
  const logs = signal(props.initialLogs ?? false);
  return <View class="flex-col gap-5">
    <BackupSettings core={props.core} fixturePath={props.backupPath} initialDialog={props.initialBackupDialog} />
    <SettingSection title="诊断">
      <SettingRow title="日志查看器" description="查看、筛选与导出应用及插件日志。">
        <Button label="日志查看器…" size="sm" onPress={() => logs.set(true)} />
      </SettingRow>
      <SettingSwitchRow divider label="开发者模式" description="开启后长按主窗口侧栏底部的“设置”打开开发工具，用于编写与调试插件、查看网络请求和日志。"
        checked={!!props.settings.value()?.developerMode}
        onChange={(developerMode) => void props.settings.update({ developerMode })} />
    </SettingSection>
    <Dialog open={logs()} onOpenChange={(e) => logs.set(e.open)}
      class="w-240 p-5 gap-4">
      <LogsView core={props.core} />
      <View class="flex-row justify-end">
        <Button label="关闭日志查看器" onPress={() => logs.set(false)} />
      </View>
    </Dialog>
    <AboutContent grouped />
  </View>;
}
/** Expansion state belongs to notice IDs, independently of recycled list cells. */
export function NotificationCenter(props: { core: Core; open: boolean; onClose: () => void }) {
  const revision = signal(0), expanded = signal<Set<number>>(new Set());
  cleanup(props.core.notifications.onChange(() => revision.update((v) => v + 1)));
  const notices = () => { revision(); return props.core.notifications.notices() as any[]; };
  return <Dialog open={props.open} onOpenChange={(e) => { if (!e.open) props.onClose(); }} class="w-128 p-5 gap-4">
    <View class="flex-row items-center gap-2">
      <Text role="header" class="flex-1 text-lg font-semibold text-foreground">通知中心</Text>
      <Button label="清除全部通知" size="sm" disabled={!notices().length} onPress={() => props.core.notifications.clearNotice(null)} />
      <Button label="关闭通知" size="sm" onPress={props.onClose} />
    </View>
    <If when={!notices().length}><Text class="text-sm text-muted-foreground">暂无通知</Text></If>
    <List each={notices()} keyExtractor={(n: any) => n.id} class="h-80 min-h-0" estimatedItemSize={90}>
      {(n: () => any) => <View class="gap-2 border-b border-border py-3">
        <View class="flex-row items-center gap-2">
          <Button label={n().message} variant="ghost" onPress={() => expanded.update((previous) => {
            const next = new Set(previous); if (next.has(n().id)) next.delete(n().id); else next.add(n().id); return next;
          })} />
          <View class="flex-1" />
          <Button label="清除" accessibilityLabel={`清除通知 ${n().id}`} size="sm"
            onPress={() => props.core.notifications.clearNotice(n().id)} />
        </View>
        <Text class="text-xs text-muted-foreground">{`${localDateTime(n().time)} · ${n().kind}`}</Text>
        <If when={expanded().has(n().id) && !!n().detail}><Text class="text-sm text-muted-foreground">{n().detail}</Text></If>
      </View>}
    </List>
  </Dialog>;
}
