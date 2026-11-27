import { List, Scroll, signal, Text, token, View } from "nsh";
import { Button as BaseButton, Tab, Tabs } from "nsh/base";
import { Button, focus } from "../../ui/Button";
import { IconButton } from "../../ui/IconButton";
import { ToggleGroup } from "../../ui/LibraryControls";
import { clearHttpExchanges, httpExchanges, type HttpExchange } from "../../state/devtools";
import { clock, CodeView, copyText, Dot, FilterField, formatBytes, KeyValues, Pane, Pill, Placeholder, prettyBody, type Tone } from "./Kit";

const filters = [{ id: "all", label: "全部" }, { id: "ok", label: "成功" }, { id: "failed", label: "失败" }] as const;
const inspectorTabs = ["响应", "请求", "标头"] as const;

function failed(x: HttpExchange) { return x.state === "failed" || (x.status ?? 0) >= 400; }
function statusTone(x: HttpExchange): Tone {
  if (x.state === "pending") return "muted";
  if (x.state === "failed" || (x.status ?? 0) >= 500) return "destructive";
  if ((x.status ?? 0) >= 400) return "warning";
  if ((x.status ?? 0) >= 300) return "primary";
  return "success";
}
function statusLabel(x: HttpExchange) {
  return x.state === "pending" ? "等待" : x.state === "failed" ? "失败" : String(x.status ?? "");
}
function split(url: string) {
  try { const u = new URL(url); return { host: u.host, path: `${u.pathname}${u.search}` || "/" }; }
  catch { return { host: "", path: url }; }
}

/** Plugin HTTP traffic recorded while this window is open; nothing is kept once it closes. */
export function NetworkPanel() {
  const query = signal(""), filter = signal<string>("all"), selected = signal<number | null>(null);
  const items = () => {
    const q = query().trim().toLowerCase();
    return httpExchanges().filter((x) => (!q || x.url.toLowerCase().includes(q) || x.sourceId.toLowerCase().includes(q))
      && (filter() === "all" || (filter() === "failed") === failed(x))).slice().reverse();
  };
  const detail = () => httpExchanges().find((x) => x.id === selected());
  return <View class="flex-1 min-h-0 flex-col gap-3" testID="network-requests">
    <View class="shrink-0 flex-row items-center gap-3">
      <FilterField class="flex-1" label="筛选请求" value={query()} placeholder="按 URL 或来源筛选" onChange={(v) => query.set(v)} />
      <ToggleGroup value={filter()} items={filters} onChange={(v) => filter.set(v)} />
      <View class="flex-row items-center gap-1.5">
        <Dot tone="success" />
        <Text class="text-xs text-muted-foreground" style={{ fontVariant: ["tabular-nums"] }}>{`记录中 · ${httpExchanges().length}`}</Text>
      </View>
      <IconButton icon="trash" label="清空请求" disabled={!httpExchanges().length}
        onPress={() => { clearHttpExchanges(); selected.set(null); }} />
    </View>
    <If when={!httpExchanges().length}>
      <Pane class="flex-1">
        <Placeholder art="network" title="等待插件发起请求"
          description="开发工具打开期间，插件发出的每个 HTTP 请求都会记录在这里，包含请求与响应的标头和正文（敏感标头已脱敏）。关闭窗口后停止记录并清空。" />
      </Pane>
    </If>
    <If when={!!httpExchanges().length}>
      <View class="flex-1 min-h-0 flex-row gap-3">
        <Pane class="flex-1" title={`${items().length} 个请求`}>
          <List each={items()} keyExtractor={(x: HttpExchange) => String(x.id)} estimatedItemSize={52} class="flex-1 min-h-0"
            empty={<Text class="p-4 text-xs text-muted-foreground">没有符合筛选条件的请求。</Text>}>
            {(x: () => HttpExchange) => <BaseButton selected={selected() === x().id} onPress={() => selected.set(x().id)}
              accessibilityLabel={`${x().method} ${x().url}`}
              class={`group gap-1 border-b border-border px-3 py-2 active:bg-muted selected:bg-selection ${focus}`}>
              <View class="flex-row items-center gap-2">
                <Text class="w-11 shrink-0 font-mono text-xs font-medium text-muted-foreground group-selected:text-selection-foreground">{x().method}</Text>
                <Text class="flex-1 min-w-0 font-mono text-xs text-foreground group-selected:text-selection-foreground"
                  style={{ numberOfLines: 1, textOverflow: "ellipsis" }}>{split(x().url).path}</Text>
                <Pill tone={statusTone(x())} label={statusLabel(x())} mono />
              </View>
              <Text class="ps-13 text-muted-foreground" style={{ fontSize: token("sidebar-hint-size"), numberOfLines: 1, fontVariant: ["tabular-nums"] }}>
                {[split(x().url).host, x().sourceId, clock(x().time, true),
                  x().elapsedMs !== undefined ? `${x().elapsedMs} ms` : "", formatBytes(x().responseSize)].filter(Boolean).join(" · ")}
              </Text>
            </BaseButton>}
          </List>
        </Pane>
        <Pane class="w-[46%]" testID="network-inspector">
          <If when={!detail()}>
            <View class="flex-1 items-center justify-center gap-1 p-6">
              <Text class="text-sm font-medium text-foreground">选择一个请求</Text>
              <Text class="text-xs text-muted-foreground">在左侧选择请求，查看响应正文、请求参数与标头。</Text>
            </View>
          </If>
          <If when={!!detail()}><Inspector exchange={detail()!} /></If>
        </Pane>
      </View>
    </If>
  </View>;
}

function Inspector(props: { exchange: HttpExchange }) {
  const tab = signal<typeof inspectorTabs[number]>("响应");
  const x = () => props.exchange;
  const body = () => tab() === "响应" ? (x().state === "failed" ? x().error ?? "" : prettyBody(x().responseBody)) : prettyBody(x().requestBody);
  return <View class="flex-1 min-h-0 flex-col">
    <View class="shrink-0 gap-1.5 border-b border-border px-3 py-2.5">
      <View class="flex-row items-center gap-2">
        <Text class="font-mono text-sm font-semibold text-foreground">{x().method}</Text>
        <Pill tone={statusTone(x())} label={statusLabel(x())} mono />
        <View class="flex-1" />
        <Text class="text-xs text-muted-foreground" style={{ fontVariant: ["tabular-nums"] }}>
          {[x().elapsedMs !== undefined ? `${x().elapsedMs} ms` : "", formatBytes(x().responseSize), x().sourceId].filter(Boolean).join(" · ")}
        </Text>
      </View>
      <View selectable><Text class="font-mono text-xs text-foreground">{x().url}</Text></View>
    </View>
    <View class="shrink-0 flex-row items-center border-b border-border px-1.5">
      <Tabs class="flex-row" indicatorClass="bottom-0 h-0.5 mx-2 rounded-full bg-primary">
        {inspectorTabs.map((name) => <Tab selected={tab() === name} onPress={() => tab.set(name)}
          class={`group px-2 py-2 ${focus}`}>
          <Text class="text-xs text-muted-foreground group-selected:font-medium group-selected:text-foreground">{name}</Text>
        </Tab>)}
      </Tabs>
      <View class="flex-1" />
      <If when={tab() !== "标头" && !!body()}>
        <Button label="复制" size="sm" variant="ghost" onPress={() => void copyText(body())} />
      </If>
    </View>
    <If when={tab() === "标头"}>
      <Scroll class="flex-1 min-h-0" contentClass="p-3">
        <Text class="pb-1 text-xs font-medium text-muted-foreground">请求标头</Text>
        <KeyValues entries={Object.entries(x().requestHeaders)} />
        <Text class="pt-4 pb-1 text-xs font-medium text-muted-foreground">响应标头</Text>
        <KeyValues entries={Object.entries(x().responseHeaders)} empty={x().state === "pending" ? "等待响应…" : "无"} />
      </Scroll>
    </If>
    <If when={tab() !== "标头"}>
      <If when={!!body()}><CodeView testID="network-body" text={body()} class={x().state === "failed" && tab() === "响应" ? "bg-destructive/5" : ""} /></If>
      <If when={!body()}>
        <View class="flex-1 items-center justify-center p-6">
          <Text class="text-xs text-muted-foreground">
            {tab() === "请求" ? "此请求没有正文。" : x().state === "pending" ? "等待响应…" : "响应没有正文。"}
          </Text>
        </View>
      </If>
    </If>
  </View>;
}
