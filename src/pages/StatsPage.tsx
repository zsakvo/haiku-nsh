import { cleanup, resource, Scroll, signal, Text, View } from "nsh";
import type { Core } from "haiku/native";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Surfaces";
import { EmptyState } from "../ui/EmptyState";
import { Tooltip } from "../ui/Surfaces";
import { usePageRefresh } from "../shell/pageRefresh";

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
  usePageRefresh(() => props.active, {
    run: () => aggregate.refetch(),
    disabled: () => aggregate.loading() || confirming(),
  });
  const maxHour = () => Math.max(1, ...(data()?.hours ?? []));
  const maxDay = () => Math.max(1, ...Object.values(data()?.days ?? {}).map(Number));
  const days = () => {
    const end = new Date();
    const monday = new Date(end.getFullYear(), end.getMonth(), end.getDate() - (end.getDay() + 6) % 7 - 77);
    return Array.from({ length: 12 }, (_, week) => Array.from({ length: 7 }, (_, weekday) => {
      const d = new Date(monday.getFullYear(), monday.getMonth(), monday.getDate() + week * 7 + weekday);
      const date = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
      return { date, value: Number(data()?.days?.[date] ?? 0), future: d > end };
    }));
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
                  tooltip={<Tooltip label={`${hour()} 时 · ${Math.round(Number(seconds()))} 秒`} />}
                  class="bg-primary rounded-(--radius-inset)"
                  style={{ height: Math.max(2, Number(seconds()) / maxHour() * 80) }} />
                <Text class="h-4 shrink-0 text-xs text-muted-foreground">{hour() % 4 === 0 ? String(hour()) : ""}</Text>
              </View>}
            </Each>
          </View>
        </View>
        <View class="gap-3">
          <Text class="text-sm font-semibold text-foreground">聆听日历 · 最近 12 周</Text>
          <View testID="stats-calendar" class="w-full flex-row gap-2">
            <View class="gap-1">
              {["一", "二", "三", "四", "五", "六", "日"].map((day) =>
                <Text class="h-6 w-4 text-xs text-muted-foreground">{day}</Text>)}
            </View>
            <Each of={days()} key={(week) => week[0].date}>{(week) =>
              <View class="flex-1 min-w-0 gap-1">
                <Each of={week()} key={(d) => d.date}>{(d) =>
                  <View role="image" accessibilityLabel={`${d().date}：${Math.round(d().value)} 秒`}
                    tooltip={<Tooltip label={`${d().date} · ${Math.round(d().value)} 秒`} />}
                    class={`w-full h-6 rounded-sm ${d().value > 0 ? "bg-primary" : "bg-muted"}`}
                    style={{ opacity: d().future ? 0 : d().value > 0 ? 0.25 + 0.75 * d().value / maxDay() : 1 }} />}
                </Each>
              </View>}
            </Each>
          </View>
          <Text class="text-xs text-muted-foreground">每列一周，颜色越深表示当天聆听越久</Text>
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
    <Dialog open={confirming()} onOpenChange={(e) => confirming.set(e.open)} class="w-96 max-w-full self-center mt-24 p-5 gap-4">
      <Text class="text-lg font-semibold text-foreground">清除本地统计？</Text>
      <Text class="text-sm text-muted-foreground">本机聆听记录将被清除，云端数据不受影响。</Text>
      <View class="flex-row justify-end gap-2">
        <Button label="取消" onPress={() => confirming.set(false)} />
        <Button label="确认清除统计" variant="destructive" onPress={() => void props.core.stats.clearStats().then(() => confirming.set(false))} />
      </View>
    </Dialog>
  </View>;
}
