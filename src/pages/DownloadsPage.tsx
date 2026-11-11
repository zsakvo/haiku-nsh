import { List, signal, Text, View } from "nsh";
import type { Core } from "haiku/native";
import { mirrorDownloads, type DownloadItem } from "../state/downloads";
import { Button } from "../ui/Button";
import { EmptyState } from "../ui/EmptyState";
import { Dialog } from "../ui/Surfaces";
import { SettingSelect, SettingSwitch } from "../ui/SettingsControls";

const labels: Record<string, string> = {
  queued: "等待中", resolving: "解析中", downloading: "下载中", tagging: "写入标签",
  completed: "已完成", failed: "失败", paused: "已暂停", canceled: "已取消", missing: "文件丢失",
};
const running = (item: DownloadItem) =>
  ["queued", "resolving", "downloading", "tagging"].includes(item.status);
const bytes = (n: number) => `${(n / 1024 ** 2).toFixed(1)} MiB`;

/** Retained virtual queue; only filters and the removal draft live in the page. */
export function DownloadsPage(props: { core: Core; active: boolean }) {
  const service = props.core.downloads;
  const items = mirrorDownloads(props.core);
  const filter = signal("all"), error = signal(""), busy = signal(false);
  const removing = signal<DownloadItem | null>(null), withFile = signal(false);
  const visible = () => items().filter(item => filter() === "all"
    || (filter() === "active" ? running(item) : item.status === filter()));
  async function act(work: () => Promise<unknown>) {
    busy.set(true); error.set("");
    try { await work(); } catch (e) { error.set(String(e)); } finally { busy.set(false); }
  }
  return <View testID="page-downloads" class="flex-1 min-h-0 pb-28"
    style={{ display: props.active ? "flex" : "none", flexDirection: "column" }}>
    <View class="flex-col px-(--content-inset) pb-4 gap-3">
      <View class="flex-row items-center gap-3">
        <Text role="header" class="flex-1 text-xl font-semibold text-foreground">下载</Text>
        <Text class="text-xs text-muted-foreground">{items().length} 项</Text>
      </View>
      <View class="flex-row gap-2 items-center flex-wrap">
        <SettingSelect label="下载状态" value={filter()} items={[
          { value: "all", title: "全部" }, { value: "active", title: "进行中" },
          ...Object.entries(labels).filter(([key]) => !["queued", "resolving", "downloading", "tagging"].includes(key))
            .map(([value, title]) => ({ value, title })),
        ]} onChange={value => filter.set(value)} />
        <Button label="暂停全部" size="sm" disabled={busy() || !items().some(running)}
          onPress={() => void act(() => Promise.all(items().filter(running).map(i => service.pause(i.key))))} />
        <Button label="继续全部" size="sm" disabled={busy() || !items().some(i => i.status === "paused")}
          onPress={() => void act(() => Promise.all(items().filter(i => i.status === "paused").map(i => service.resume(i.key))))} />
        <Button label="重试失败" size="sm" disabled={busy() || !items().some(i => ["failed", "missing"].includes(i.status))}
          onPress={() => void act(() => service.retryFailed())} />
        <Button label="清理已完成记录" size="sm" disabled={busy() || !items().some(i => i.status === "completed")}
          onPress={() => void act(() => service.clearCompleted())} />
        <Button label="校验文件" size="sm" disabled={busy()}
          onPress={() => void act(() => service.verify())} />
      </View>
      <Text class="text-xs text-muted-foreground">清理已完成记录会保留磁盘文件。</Text>
      <If when={!!error()}><Text class="text-sm text-destructive">{error()}</Text></If>
    </View>
    <List each={visible()} keyExtractor={item => item.key} estimatedItemSize={104}
      class="flex-1 min-h-0" testID="download-list">
      {(item) => <View class="flex-col px-(--content-inset) py-3 gap-2 border-b border-border">
        <View class="flex-row items-center gap-3">
          <View class="flex-1 min-w-0 flex-col gap-1 items-start">
            <Text class="text-sm font-medium text-foreground">{item().track.title}</Text>
            <Text class="text-xs text-muted-foreground">
              {item().track.artists.map(a => typeof a === "string" ? a : a.name).join(" / ")}
            </Text>
          </View>
          <Text class="text-xs text-muted-foreground">{labels[item().status] ?? item().status}</Text>
          <If when={running(item())}>
            <Button label="暂停" accessibilityLabel={`暂停 ${item().track.title}`} size="sm" disabled={busy()}
              onPress={() => void act(() => service.pause(item().key))} />
            <Button label="取消" accessibilityLabel={`取消 ${item().track.title}`} size="sm" disabled={busy()}
              onPress={() => void act(() => service.cancel(item().key))} />
          </If>
          <If when={["paused", "canceled"].includes(item().status)}>
            <Button label="继续" accessibilityLabel={`继续 ${item().track.title}`} size="sm" disabled={busy()}
              onPress={() => void act(() => service.resume(item().key))} />
          </If>
          <If when={["failed", "missing"].includes(item().status)}>
            <Button label="重试" accessibilityLabel={`重试 ${item().track.title}`} size="sm" disabled={busy()}
              onPress={() => void act(() => service.retry(item().key))} />
          </If>
          <If when={item().status === "completed"}>
            <Button label="显示文件" accessibilityLabel={`显示文件 ${item().track.title}`} size="sm" disabled={busy()}
              onPress={() => void act(() => service.reveal(item().key))} />
          </If>
          <Button label="移除…" accessibilityLabel={`移除 ${item().track.title}`} size="sm" disabled={busy()}
            onPress={() => { withFile.set(false); removing.set(item()); }} />
        </View>
        <Text class="text-xs text-muted-foreground">
          {`${bytes(item().received)}${item().total ? ` / ${bytes(item().total!)}` : ""} · ${item().actualQuality ?? item().quality}`}
        </Text>
        <If when={running(item()) || item().status === "paused"}>
          <View class="h-1 rounded-full bg-muted overflow-hidden">
            <View class="h-1 bg-primary" style={{
              width: `${Math.min(100, 100 * item().received / Math.max(1, item().total ?? item().received))}%`,
            }} />
          </View>
        </If>
        <If when={!!item().error}><Text class="text-xs text-destructive">{item().error}</Text></If>
        <If when={!!item().warning}><Text class="text-xs text-muted-foreground">{item().warning}</Text></If>
      </View>}
    </List>
    <If when={!visible().length}><EmptyState title="没有下载任务"
      description="在曲目菜单中选择“下载”，或在歌单与专辑中选择“下载全部”。" /></If>
    <Dialog open={!!removing()} onOpenChange={e => { if (!e.open && !busy()) removing.set(null); }}
      class="w-112 p-5 gap-4">
      <Text role="header" class="text-lg font-semibold text-foreground">移除下载？</Text>
      <Text class="text-sm text-muted-foreground">{removing()?.track.title}</Text>
      <SettingSwitch label="同时删除音频与附加文件" checked={withFile()} onChange={v => withFile.set(v)} />
      <Text class="text-xs text-muted-foreground">
        {withFile() ? "将删除音频、未完成文件与封面/歌词侧车，无法撤销。" : "仅移除下载记录，已下载的文件会保留。"}
      </Text>
      <If when={!!error()}><Text class="text-sm text-destructive">{error()}</Text></If>
      <View class="flex-row justify-end gap-2">
        <Button label="取消移除" disabled={busy()} onPress={() => removing.set(null)} />
        <Button label="确认移除" variant="destructive" loading={busy()} onPress={() => void act(async () => {
          await service.remove(removing()!.key, withFile()); removing.set(null);
        })} />
      </View>
    </Dialog>
  </View>;
}
