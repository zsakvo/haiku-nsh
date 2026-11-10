import { cleanup, Clipboard, List, resource, signal, Text, View } from "nsh";
import type { Core } from "haiku/native";
import { Button } from "../ui/Button";
import { SearchField } from "../ui/SearchField";
import { Dialog, Menu, MenuItem } from "../ui/Surfaces";

/** Filtered pages and redaction come from Rust; copying and saving are explicit actions. */
export function LogsView(props: { core: Core }) {
  const service = props.core.logs;
  const revision = signal(0), channel = signal(""), level = signal("");
  const source = signal(""), query = signal(""), offset = signal(0);
  const confirming = signal(false), error = signal("");
  cleanup(service.onChange(() => revision.update((v) => v + 1)));
  const filter = () => ({ channel: channel(), level: level(), source: source(), query: query() });
  const result = resource(
    () => ({ revision: revision(), filter: filter(), offset: offset() }),
    (source) => service.page(source.filter, source.offset, 50),
  );
  const data = () => result() as { total: number; items: any[] } | undefined;
  async function act(action: () => Promise<unknown>) {
    try { await action(); error.set(""); } catch (e) { error.set(String(e)); }
  }
  return <View testID="logs-viewer" class="gap-3">
    <Text role="header" class="text-lg font-semibold text-foreground">日志查看器</Text>
    <View class="flex-row gap-2 flex-wrap">
      <Menu trigger={<Button label={`频道：${channel() || "全部"}`} size="sm" onPress={() => {}} />}>
        {["", "app", "plugin"].map((v) => <MenuItem onSelect={() => { channel.set(v); offset.set(0); }}><Text>{v || "全部频道"}</Text></MenuItem>)}
      </Menu>
      <Menu trigger={<Button label={`级别：${level() || "全部"}`} size="sm" onPress={() => {}} />}>
        {["", "info", "warn", "error"].map((v) => <MenuItem onSelect={() => { level.set(v); offset.set(0); }}><Text>{v || "全部级别"}</Text></MenuItem>)}
      </Menu>
      <Button label="复制筛选结果" size="sm" onPress={() => void act(async () => Clipboard.writeText(await service.copyText(filter())))} />
      <Button label="导出全部日志…" size="sm" onPress={() => void act(() => service.exportLogs())} />
      <Button label="清空日志…" size="sm" onPress={() => confirming.set(true)} />
    </View>
    <View class="flex-row gap-2">
      <View class="flex-1"><SearchField label="日志关键词" value={query()} placeholder="搜索消息或详情" onChange={(v) => { query.set(v); offset.set(0); }} /></View>
      <View class="w-40"><SearchField label="日志来源" value={source()} placeholder="来源精确匹配" onChange={(v) => { source.set(v); offset.set(0); }} /></View>
    </View>
    <If when={!!error() || !!result.error()}><Text class="text-sm text-destructive">{error() || String(result.error())}</Text></If>
    <Text class="text-xs text-muted-foreground">{`共 ${data()?.total ?? 0} 条日志`}</Text>
    <List each={data()?.items ?? []} keyExtractor={(e: any, index) => `${e.time}:${e.session}:${index}`}
      class="h-72 min-h-0 rounded-(--radius) border border-border" estimatedItemSize={80}>
      {(e: () => any) => <View class="gap-1 px-3 py-3 border-b border-border">
        <Text class="text-xs text-muted-foreground">{`${e().time} · ${e().channel}/${e().level} · ${e().source} · 会话 ${e().session}`}</Text>
        <Text class="text-sm text-foreground">{e().message}</Text>
        <If when={!!e().detail}><Text class="text-xs text-muted-foreground">{e().detail}</Text></If>
      </View>}
    </List>
    <View class="flex-row gap-2 items-center">
      <Button label="上一页" size="sm" disabled={offset() === 0} onPress={() => offset.set(Math.max(0, offset() - 50))} />
      <Text class="text-xs text-muted-foreground">{`第 ${Math.floor(offset() / 50) + 1} 页`}</Text>
      <Button label="下一页" size="sm" disabled={offset() + 50 >= (data()?.total ?? 0)} onPress={() => offset.set(offset() + 50)} />
    </View>
    <Dialog open={confirming()} onOpenChange={(e) => confirming.set(e.open)} class="w-96 p-5 gap-4">
      <Text class="text-lg font-semibold text-foreground">清空日志？</Text>
      <Text class="text-sm text-muted-foreground">{channel() ? `清空 ${channel()} 频道全部日志。` : "清空 app 和 plugin 两个频道的全部日志。"}</Text>
      <View class="flex-row justify-end gap-2">
        <Button label="取消" onPress={() => confirming.set(false)} />
        <Button label="确认清空日志" variant="destructive" onPress={() => void act(async () => {
          await service.clearLogs(channel()); offset.set(0); confirming.set(false);
        })} />
      </View>
    </Dialog>
  </View>;
}
/** Logs and software identification live in Settings → Other. */
export function OtherSettings(props: { core: Core; initialLogs?: boolean }) {
  const logs = signal(props.initialLogs ?? false);
  return <View class="gap-5">
    <Text role="header" class="text-xl font-semibold text-foreground">其他</Text>
    <Button label="日志查看器…" onPress={() => logs.set(!logs())} />
    <If when={logs()}><LogsView core={props.core} /></If>
    <View class="gap-2">
      <Text class="text-lg font-semibold text-foreground">关于 Haiku</Text>
      <Text class="text-sm text-muted-foreground">Haiku 1.0.0 · Native Shell 0.4.0</Text>
      <Text class="text-sm text-muted-foreground">应用许可证尚未声明。Iconoir 图标使用 MIT 许可证，插画使用 unDraw 许可；授权文件随资源提供。</Text>
    </View>
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
          <Button label={`清除通知 ${n().id}`} size="sm" onPress={() => props.core.notifications.clearNotice(n().id)} />
        </View>
        <Text class="text-xs text-muted-foreground">{`${n().time} · ${n().kind}`}</Text>
        <If when={expanded().has(n().id) && !!n().detail}><Text class="text-sm text-muted-foreground">{n().detail}</Text></If>
      </View>}
    </List>
  </Dialog>;
}
