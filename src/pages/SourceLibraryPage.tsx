import { cleanup, resource, Scroll, signal, Text, View } from "nsh";
import type { Core } from "haiku/native";
import type { Route } from "../shell/navigation";
import { Button } from "../ui/Button";
import { SourceGrid, SourceTracks } from "./SourceContent";
import { usePageRefresh } from "../shell/pageRefresh";

/** Descriptor cards own each group's cursor. A failed connection leaves a direct settings path. */
export function SourceLibraryPage(props: { core: Core; source: string; title?: string; active: boolean; navigate: (route: Route) => void; settings: () => void }) {
  const groups = signal<any[]>([]), error = signal(""), busy = signal("");
  let version = 0, closed = false;
  cleanup(() => { closed = true; version++; });
  const data = resource(() => props.active ? props.source : undefined, async (source) => {
    const current = ++version;
    const result = await props.core.sources.browse(source, null, null) as any;
    if (!closed && current === version && props.active) groups.set(result.groups);
  });
  async function more(group: any) {
    const current = version;
    busy.set(group.card.id); error.set("");
    try {
      const result = await props.core.sources.browse(props.source, group.card.id, group.page.next) as any;
      if (closed || current !== version || !props.active) return;
      const page = result.groups[0].page;
      groups.set(groups().map((g) => g.card.id === group.card.id ? {...g,page:{...page,items:[...g.page.items,...page.items]}} : g));
    } catch (e) { error.set(String(e)); } finally { busy.set(""); }
  }
  usePageRefresh(() => props.active, {
    run: () => { error.set(""); data.refetch(); },
    disabled: () => data.loading() || !!busy(),
  });
  return <View testID="page-source-library" class="flex-1 min-h-0 pb-28"
    style={{ display: props.active ? "flex" : "none", flexDirection: "column" }}>
    <Scroll testID="source-library-scroll" class="flex-1" contentClass="flex-col px-(--content-inset) gap-3 pb-4">
      <Text role="header" class="text-xl font-semibold text-foreground">{props.title || "来源音乐库"}</Text>
      <If when={data.loading() && !groups().length}><View class="h-40 bg-muted rounded-(--radius-panel)" accessibilityLabel="正在加载音乐库" /></If>
      <If when={!!data.error() || !!error()}>
        <Text class="text-sm text-destructive">{error() || String(data.error())}</Text>
        <View class="flex-row gap-2">
          <Button label="重试连接" size="sm" onPress={() => { error.set(""); data.refetch(); }} />
          <Button label="去设置…" size="sm" onPress={props.settings} />
        </View>
      </If>
      <Each of={groups()} key={(g) => g.card.id}>{(group) => <View class="gap-2">
        <Text role="header" class="text-lg font-semibold text-foreground">{group().card.label}</Text>
        <If when={group().card.entity === "track"}><View class="h-64"><SourceTracks core={props.core} tracks={group().page.items} active={props.active} /></View></If>
        <If when={group().card.entity !== "track"}><SourceGrid items={group().page.items} navigate={props.navigate} /></If>
        <If when={!!group().page.next}><View class="flex-row"><Button label={`加载更多${group().card.label}`} size="sm" loading={busy() === group().card.id}
          onPress={() => void more(group())} /></View></If>
        <If when={!group().page.items.length}><Text class="text-sm text-muted-foreground">此分组暂无内容</Text></If>
      </View>}</Each>
    </Scroll>
  </View>;
}
