import { cleanup, resource, Scroll, signal, Text, View } from "nsh";
import type { Core } from "haiku/native";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Surfaces";
import { EmptyState } from "../ui/EmptyState";

/** Small bounded charts use TSX views and semantic token colors; Rust performs all aggregation. */
export function StatsPage(props: { core: Core; active: boolean }) {
  const range = signal("all");
  const revision = signal(0);
  const confirming = signal(false);
  const ranking = signal("tracks");
  cleanup(props.core.stats.onChange(() => revision.update((v) => v + 1)));
  const aggregate = resource(
    () => ({ revision: revision(), range: range() }),
    (source) => props.core.stats.aggregate(source.range),
  );
  const data = () => aggregate() as any;
  const maxHour = () => Math.max(1, ...(data()?.hours ?? []));
  const days = () => {
    const end = new Date();
    const result: { date: string; value: number }[] = [];
    for (let i = 83; i >= 0; i--) {
      const d = new Date(end.getFullYear(), end.getMonth(), end.getDate() - i);
      const date = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
      result.push({ date, value: data()?.days?.[date] ?? 0 });
    }
    return result;
  };
  return <View testID="page-statistics" class="flex-1 min-h-0 pb-28"
    style={{ display: props.active ? "flex" : "none", flexDirection: "column" }}>
    <Scroll class="flex-1" contentClass="flex-col px-(--content-inset) gap-5 pb-5">
      <View class="flex-row items-center gap-2 flex-wrap">
        <Text role="header" class="flex-1 text-xl font-semibold text-foreground">统计</Text>
        {[["day", "今日"], ["month", "本月"], ["year", "今年"], ["all", "全部"]].map(([id, label]) =>
          <Button label={label} size="sm" variant={range() === id ? "primary" : "secondary"} onPress={() => range.set(id)} />)}
        <Button label="清除本地统计…" size="sm" onPress={() => confirming.set(true)} />
      </View>
      <If when={!!aggregate.error()}><Text class="text-sm text-destructive">{String(aggregate.error())}</Text></If>
      <If when={!!data()}>
        <View class="flex-row gap-3 flex-wrap">
          {[
            ["聆听时长", () => `${Math.round(data()?.seconds / 60)} 分钟`],
            ["播放次数", () => `${data()?.count} 次`],
            ["歌曲", () => `${data()?.uniqueTracks} 首`],
            ["歌手", () => `${data()?.uniqueArtists} 位`],
          ].map(([label, value]) => <View class="flex-1 min-w-32 gap-2 rounded-(--radius-panel) bg-muted p-4">
            <Text class="text-xs text-muted-foreground">{label as string}</Text>
            <Text class="text-xl font-semibold text-foreground">{(value as () => string)()}</Text>
          </View>)}
        </View>
        <If when={data()?.count === 0}><EmptyState title="从五秒开始" description="连续播放至少五秒，暂停或切歌后便会记录一次聆听。" /></If>
        <View class="gap-3">
          <Text class="text-sm font-semibold text-foreground">一天中的聆听</Text>
          <View testID="stats-hours" class="h-28 flex-row gap-1 items-end">
            <Each of={data()?.hours ?? []}>{(seconds, hour) =>
              <View class="flex-1 h-full justify-end gap-1">
                <View role="image" accessibilityLabel={`${hour()} 时：${Math.round(Number(seconds()))} 秒`}
                  class="bg-primary rounded-(--radius-inset)"
                  style={{ height: Math.max(2, Number(seconds()) / maxHour() * 80) }} />
                <Text class="text-xs text-muted-foreground">{hour() % 4 === 0 ? String(hour()) : ""}</Text>
              </View>}
            </Each>
          </View>
        </View>
        <View class="gap-3">
          <Text class="text-sm font-semibold text-foreground">聆听日历 · 最近 12 周</Text>
          <View testID="stats-calendar" class="flex-row flex-wrap gap-1 max-w-xl">
            <Each of={days()} key={(d) => d.date}>{(d) =>
              <View role="image" accessibilityLabel={`${d().date}：${Math.round(d().value)} 秒`}
                class={`size-5 rounded-(--radius-inset) ${d().value > 0 ? "bg-primary" : "bg-muted"}`}
                style={{ opacity: d().value > 0 ? Math.min(1, 0.35 + d().value / 120) : 1 }} />}
            </Each>
          </View>
          <Text class="text-xs text-muted-foreground">{`当前连续 ${data()?.currentStreak} 天 · 最长连续 ${data()?.longestStreak} 天`}</Text>
        </View>
        <View class="gap-2">
          <View class="flex-row gap-2">
            <Button label="歌曲排行" size="sm" variant={ranking() === "tracks" ? "primary" : "secondary"} onPress={() => ranking.set("tracks")} />
            <Button label="歌手排行" size="sm" variant={ranking() === "artists" ? "primary" : "secondary"} onPress={() => ranking.set("artists")} />
          </View>
          <Each of={(data()?.[ranking()] ?? []).slice(0, 10)}>{(entry, index) =>
            <View class="flex-row gap-3 py-2 border-b border-border">
              <Text class="w-6 text-sm text-muted-foreground">{index() + 1}</Text>
              <Text class="flex-1 text-sm text-foreground">{(entry() as any).track?.title ?? (entry() as any).name}</Text>
              <Text class="text-sm text-muted-foreground">{`${(entry() as any).count} 次 · ${Math.round((entry() as any).seconds / 60)} 分钟`}</Text>
            </View>}
          </Each>
        </View>
        <View class="gap-2">
          <Text class="text-sm font-semibold text-foreground">聆听里程碑</Text>
          <View class="flex-row flex-wrap gap-2">
            <Each of={data()?.milestones ?? []}>{(m) =>
              <Text class={`rounded-(--radius) px-3 py-2 text-xs ${(m() as any).reached ? "bg-primary text-primary-foreground" : "bg-muted text-muted-foreground"}`}>
                {`${(m() as any).reached ? "✓ " : ""}${(m() as any).count} 次`}
              </Text>}
            </Each>
          </View>
        </View>
      </If>
    </Scroll>
    <Dialog open={confirming()} onOpenChange={(e) => confirming.set(e.open)} class="w-96 p-5 gap-4">
      <Text class="text-lg font-semibold text-foreground">清除本地统计？</Text>
      <Text class="text-sm text-muted-foreground">本机聆听记录将被清除，云端数据不受影响。</Text>
      <View class="flex-row justify-end gap-2">
        <Button label="取消" onPress={() => confirming.set(false)} />
        <Button label="确认清除统计" variant="destructive" onPress={() => void props.core.stats.clearStats().then(() => confirming.set(false))} />
      </View>
    </Dialog>
  </View>;
}
