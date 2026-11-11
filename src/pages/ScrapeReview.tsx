import { List, Scroll, signal, Text, View } from "nsh";
import type { Core } from "haiku/native";
import { mirrorScrape, type ScrapeCandidate } from "../state/scrape";
import { Button } from "../ui/Button";
import { Field } from "../ui/LibraryControls";
import { Dialog } from "../ui/Surfaces";
import { createPlugins } from "../state/plugins";

const labels: Record<string, string> = {
  pending: "等待中", searching: "查找中", applied: "已采用", review: "待审阅",
  none: "无匹配", skipped: "已跳过", error: "失败", auto: "自动匹配",
};
/** One review selection, a virtual job list and candidate field comparisons. */
export function ScrapeReview(props: { core: Core; open: boolean; onClose: () => void }) {
  const snapshot = mirrorScrape(props.core);
  const plugins = createPlugins();
  const providerName = (id: string) => plugins.value().sources.find(s => s.sourceId === id)?.displayName
    || plugins.value().packages.find(p => p.id === id)?.name
    || plugins.value().packages.find(p => p.id === plugins.value().sources.find(s => s.sourceId === id)?.pluginId)?.name
    || "未知来源";
  const selected = signal(""), index = signal(0), keyword = signal("");
  const error = signal(""), busy = signal(false);
  const item = () => snapshot().items.find(i => i.track.id === selected())
    ?? snapshot().items.find(i => i.status === "review") ?? snapshot().items[0];
  const candidate = () => item()?.candidates[index()];
  const current = () => item() ? props.core.library.track(item()!.track.id) as any : null;
  const artistNames = (artists: any[]) => (artists ?? []).map(a => typeof a === "string" ? a : a.name).join(" / ");
  function changes(c: ScrapeCandidate) {
    const t = current();
    return [
      ["标题", t?.title, c.title], ["艺术家", artistNames(t?.artists), c.artists.join(" / ")],
      ["专辑", t?.album?.title, c.album], ["年份", t?.extra?.year, c.year],
      ["曲号", t?.extra?.track_no, c.trackNo], ["封面", t?.cover, c.cover],
    ].filter(([, old, next]) => next != null && String(next).trim() !== "" && next !== old);
  }
  async function act(work: () => Promise<unknown>) {
    busy.set(true); error.set("");
    try { await work(); } catch (e) { error.set(String(e)); } finally { busy.set(false); }
  }
  return <Dialog open={props.open} onOpenChange={e => { if (!e.open) props.onClose(); }}
    class="w-240 max-w-full self-center mt-10 p-5 gap-4">
    <View class="flex-row items-center gap-3">
      <Text role="header" class="flex-1 text-lg font-semibold text-foreground">刮削审阅</Text>
      <Button label="关闭审阅" variant="ghost" size="sm" onPress={props.onClose} />
    </View>
    <View class="flex-row items-center gap-3">
      <Text class="flex-1 text-sm text-muted-foreground">
        {`${snapshot().running ? "正在刮削" : "刮削完成"} · ${snapshot().completed} / ${snapshot().total}`}
      </Text>
      <If when={snapshot().running}>
        <Button label="取消刮削" size="sm" onPress={() => props.core.scrape.cancel()} />
      </If>
    </View>
    <Text class="text-xs text-muted-foreground">采用仅保存应用内元数据。写回音乐文件请在编辑信息中单独确认。</Text>
    <View class="h-104 min-h-0 flex-row gap-4">
      <List testID="scrape-review-items" each={snapshot().items} keyExtractor={i => i.track.id}
        estimatedItemSize={68} class="w-56 shrink-0 min-h-0">
        {entry => <Button label={`${entry().track.title} · ${labels[entry().status] ?? entry().status}${entry().automatic ? " · 自动" : ""}`}
          class="items-start justify-start"
          variant={item()?.track.id === entry().track.id ? "secondary" : "ghost"}
          onPress={() => { selected.set(entry().track.id); index.set(0); keyword.set(entry().track.title); }} />}
      </List>
      <View class="flex-1 min-w-0 gap-3 border-l border-border pl-4">
        <View class="flex-row gap-2 items-end">
          <View class="flex-1"><Field label="手动匹配关键词" value={keyword()} placeholder={item()?.track.title}
            onChange={value => keyword.set(value)} /></View>
          <Button label="重新匹配" size="sm" disabled={!item() || snapshot().running || !keyword().trim() || busy()}
            onPress={() => void act(() => props.core.scrape.retry(item()!.track.id, keyword()))} />
        </View>
        <List testID="scrape-candidates" each={item()?.candidates ?? []} keyExtractor={c => `${c.providerId}:${c.id}`}
          estimatedItemSize={44} class="h-24 shrink-0">
          {c => <Button class="items-start justify-start" label={`${c().title} · ${Math.round(c().score * 100)} 分 · ${providerName(c().providerId)}`}
            variant={candidate()?.id === c().id ? "secondary" : "ghost"}
            onPress={() => index.set(item()!.candidates.findIndex(value =>
              value.id === c().id && value.providerId === c().providerId))} />}
        </List>
        <Text class="text-sm font-semibold text-foreground">字段差异</Text>
        <Scroll testID="scrape-differences" class="flex-1 min-h-0" contentClass="gap-3">
          <Each of={candidate() ? changes(candidate()!) : []} key={row => String(row[0])}>{row =>
            <View class="gap-1">
              <Text class="text-xs text-muted-foreground">{String(row()[0])}</Text>
              <Text class="text-sm text-muted-foreground">{String(row()[1] ?? "未填写")}</Text>
              <Text class="text-sm text-foreground">→ {String(row()[2])}</Text>
            </View>}
          </Each>
          <If when={!!item()?.error}><Text class="text-xs text-destructive">{item()?.error}</Text></If>
        </Scroll>
        <View class="flex-row justify-end gap-2">
          <Button label="跳过" size="sm" disabled={!item() || busy() || ["applied", "pending", "searching"].includes(item()!.status)}
            onPress={() => void act(() => props.core.scrape.skip(item()!.track.id))} />
          <If when={item()?.status === "applied"}>
            <Button label="撤销采用" size="sm" disabled={busy()}
              onPress={() => void act(() => props.core.scrape.undo(item()!.track.id))} />
          </If>
          <If when={item()?.status !== "applied"}>
            <Button label="采用候选" size="sm" variant="primary" disabled={!candidate() || busy()}
              onPress={() => void act(() => props.core.scrape.accept(item()!.track.id, index()))} />
          </If>
        </View>
      </View>
    </View>
    <If when={!!error()}><Text class="text-sm text-destructive">{error()}</Text></If>
  </Dialog>;
}
