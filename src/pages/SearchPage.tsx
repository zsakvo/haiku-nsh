import { cleanup, effect, Image, List, resource, Scroll, signal, Text, token, untracked, View } from "nsh";
import { Button as BaseButton } from "nsh/base";
import type { Core } from "haiku/native";
import type { PluginSnapshot } from "../state/plugins";
import { searchableSources, type SearchKind, type SearchPageResult, type SearchRequest } from "../state/search";
import type { Route } from "../shell/navigation";
import { Button, focus } from "../ui/Button";
import { Spinner } from "../ui/Discovery";
import { EmptyState } from "../ui/EmptyState";
import { Icon, type IconName } from "../ui/Icon";
import { Dialog } from "../ui/Surfaces";
import { entityRoute, SourceTracks } from "./SourceContent";
import { usePageRefresh } from "../shell/pageRefresh";
import { usePageSource } from "../shell/pageSource";

function entityArtists(entity: any): string {
  return (entity.artists ?? [])
    .map((a: string | { name: string }) => (typeof a === "string" ? a : a.name))
    .filter(Boolean)
    .join(" / ");
}

function entitySubtitle(entity: any, kind: SearchKind): string {
  const artists = entityArtists(entity);
  if (kind === "album") return artists || entity.subtitle || entity.description || "";
  if (kind === "playlist") return entity.creator || entity.subtitle || entity.description || "";
  if (kind === "program") return entity.host || entity.creator || entity.subtitle || entity.description || "";
  return entity.subtitle || entity.description || artists || "";
}

function entityMetadata(entity: any): string {
  return [
    entity.track_count != null ? `${entity.track_count} 首` : null,
    entity.episode_count != null ? `${entity.episode_count} 期` : null,
    entity.year != null ? String(entity.year) : null,
  ].filter(Boolean).join(" · ");
}

function entityFallbackIcon(kind: SearchKind): IconName {
  if (kind === "artist") return "user";
  if (kind === "playlist") return "playlist";
  if (kind === "program") return "radio";
  return "music";
}

function entityColumnHeading(kind: SearchKind): string {
  if (kind === "album") return "专辑";
  if (kind === "artist") return "歌手";
  if (kind === "playlist") return "歌单";
  if (kind === "program") return "节目";
  return "名称";
}

/** One retained search workspace; every response is checked against the full current query identity. */
export function SearchPage(props: {
  core: Core;
  active: boolean;
  snapshot: PluginSnapshot;
  request?: SearchRequest;
  navigate: (route: Route) => void;
  settings: () => void;
}) {
  const query = signal(props.request?.query ?? "");
  const kind = signal<SearchKind>(props.request?.kind ?? "track");
  const source = signal(props.request?.source ?? (props.core.settings.get() as any).searchSourceId ?? "");
  const note = signal("");
  const results = signal<any[]>([]);
  const next = signal<string | null>(null);
  const error = signal("");
  const moreBusy = signal(false);
  const wantMore = signal(false);
  const cancelled = signal(false);
  const confirm = signal(false);
  const histories = signal<string[]>((props.core.history.list() as any).searches ?? []);
  const stopHistory = props.core.history.onChange(() => {
    histories.set((props.core.history.list() as any).searches ?? []);
  });

  const available = () => searchableSources(props.snapshot);
  const types = () => available().find((s) => s.sourceId === source())?.kinds ?? [];
  let epoch = 0;
  let closed = false;
  cleanup(() => {
    closed = true;
    epoch++;
    stopHistory();
  });

  effect(() => {
    const sources = available();
    if (!sources.some((s) => s.sourceId === source())) source.set(sources[0]?.sourceId ?? "");
    const supported = types();
    if (supported.length && !supported.some((t) => t.id === kind())) kind.set(supported[0].id);
  });

  effect(() => {
    const request = props.request;
    if (request) {
      cancelled.set(false);
      query.set(request.query);
      if (request.source) source.set(request.source);
      if (request.kind) kind.set(request.kind);
    }
  });

  const identity = () =>
    props.active && source() && query().trim() && !cancelled()
      ? JSON.stringify([source(), kind(), query().trim()])
      : undefined;

  const first = resource(identity, async (key) => {
    const version = ++epoch;
    results.set([]);
    next.set(null);
    error.set("");
    moreBusy.set(false);
    wantMore.set(false);
    const [s, k, q] = JSON.parse(key);
    const page = (await props.core.sources.search(s, k, q, null)) as SearchPageResult;
    if (closed || version !== epoch || identity() !== key) return;
    results.set(page.items);
    next.set(page.next ?? null);
  });

  function changeQuery(value: string) {
    epoch++;
    cancelled.set(false);
    query.set(value);
    results.set([]);
    next.set(null);
    error.set("");
    note.set("");
    moreBusy.set(false);
    wantMore.set(false);
  }

  function submit(value = query()) {
    const q = value.trim();
    changeQuery(q);
    if (q) props.core.history.search(q);
    first.refetch();
  }

  function cancelSearch() {
    epoch++;
    cancelled.set(true);
    moreBusy.set(false);
    wantMore.set(false);
  }

  function switchKind(nextKind: SearchKind) {
    if (kind() === nextKind) return;
    epoch++;
    cancelled.set(false);
    note.set("");
    error.set("");
    results.set([]);
    next.set(null);
    moreBusy.set(false);
    wantMore.set(false);
    kind.set(nextKind);
  }

  usePageRefresh(() => props.active, {
    run: () => {
      cancelled.set(false);
      error.set("");
      first.refetch();
    },
    disabled: () => !identity() || first.loading() || moreBusy() || confirm(),
  });

  async function selectSource(value: string) {
    epoch++;
    cancelled.set(false);
    wantMore.set(false);
    source.set(value);
    error.set("");
    note.set("");
    try {
      await props.core.settings.update({ searchSourceId: value });
    } catch (e) {
      error.set(String(e));
    }
  }

  usePageSource({
    active: () => props.active,
    value: source,
    items: () =>
      available().map((s) => ({
        id: s.sourceId,
        title: s.title,
        logo: props.snapshot.packages.find((p) => p.id === s.pluginId)?.logo,
      })),
    select: (value) => void selectSource(value),
  });

  async function more() {
    const key = identity();
    const version = epoch;
    const cursor = next();
    if (!key || !cursor || moreBusy()) return;
    moreBusy.set(true);
    error.set("");
    try {
      const [s, k, q] = JSON.parse(key);
      const page = (await props.core.sources.search(s, k, q, cursor)) as SearchPageResult;
      if (closed || version !== epoch || identity() !== key) return;
      results.set([...results(), ...page.items]);
      next.set(page.next ?? null);
    } catch (e) {
      if (!closed && version === epoch && identity() === key) error.set(String(e));
    } finally {
      if (!closed && version === epoch && identity() === key) moreBusy.set(false);
    }
  }

  effect(() => {
    if (wantMore() && props.active && !!next() && !first.loading() && !moreBusy() && !error() && !cancelled()) {
      wantMore.set(false);
      untracked(() => void more());
    }
  });

  function openEntity(route: Route) {
    if (
      route.kind === "program" &&
      !props.snapshot.packages
        .find((p) => p.id === available().find((s) => s.sourceId === source())?.pluginId)
        ?.capabilities.includes("program.get")
    ) {
      note.set("此来源暂不提供节目详情，请选择其他结果。");
      return;
    }
    note.set("");
    props.navigate(route);
  }

  const paginationFooter = () => (
    <View class="py-4 items-center justify-center gap-2">
      <If when={moreBusy()}>
        <Spinner label="正在加载更多…" />
      </If>
      <If when={!!next() && !moreBusy()}>
        <Button label="加载更多结果" size="sm" variant="ghost" onPress={() => void more()} />
      </If>
      <If when={!next() && !moreBusy() && results().length > 0}>
        <Text class="text-xs text-muted-foreground">已显示全部结果</Text>
      </If>
    </View>
  );

  return (
    <View
      testID="page-search"
      class="flex-1 min-h-0"
      style={{ display: props.active ? "flex" : "none", flexDirection: "column" }}
    >
      <View class="shrink-0 pl-(--content-inset-left) pr-(--content-inset-right) pt-0 pb-5 gap-3">
        <View testID="search-title" class="flex-row items-baseline gap-1.5 min-w-0">
          <Text
            role="header"
            class="shrink-0 font-semibold text-foreground"
            style={{ fontSize: 24 }}
          >
            搜索
          </Text>
          <If when={!!query().trim()}>
            <Text
              class="min-w-0 shrink font-semibold text-primary"
              style={{ fontSize: 24, numberOfLines: 1, textOverflow: "ellipsis" }}
            >
              {query().trim()}
            </Text>
          </If>
        </View>

        <If when={types().length > 0}>
          <View class="flex-row items-center">
            <View
              testID="search-tabs"
              class="shrink-0 flex-row items-center gap-0.5 rounded-(--radius) border border-border bg-sidebar p-0.5"
            >
              <Each of={types()} key={(t) => t.id}>
                {(t) => (
                  <BaseButton
                    accessibilityLabel={t().label}
                    selected={kind() === t().id}
                    onPress={() => switchKind(t().id)}
                    class={`group min-h-7 items-center justify-center rounded-(--radius-inset) px-3 py-1 selected:bg-popover selected:shadow-sm ${focus}`}
                  >
                    <Text class="text-xs text-muted-foreground group-selected:text-foreground group-selected:font-medium">
                      {t().label}
                    </Text>
                  </BaseButton>
                )}
              </Each>
            </View>
          </View>
        </If>

        <If when={!!note()}>
          <Text class="text-xs text-muted-foreground">{note()}</Text>
        </If>
      </View>

      <If when={!available().length}>
        <EmptyState
          title="还没有可用的搜索来源"
          description="在设置中启用支持搜索功能的音乐来源。"
          action={{ label: "管理来源…", onPress: props.settings }}
        />
      </If>

      {/* Search history when query is empty */}
      <If when={available().length > 0 && !query().trim()}>
        <View class="flex-1 min-h-0 pl-(--content-inset-left) pr-(--content-inset-right) pt-1">
          <View class="flex-row items-center justify-between gap-3 pb-3">
            <Text role="header" class="text-sm font-semibold text-foreground">
              搜索历史
            </Text>
            <If when={histories().length > 0}>
              <Button
                label="清空搜索历史…"
                size="sm"
                variant="ghost"
                onPress={() => confirm.set(true)}
              />
            </If>
          </View>

          <If when={histories().length > 0}>
            <Scroll
              testID="search-history"
              class="flex-1 min-h-0"
              contentClass="flex-row flex-wrap gap-2 pb-(--dock-scroll-clearance)"
            >
              <Each of={histories()} key={(q) => q}>
                {(q) => (
                  <View class="flex-row items-center rounded-full border border-border bg-card overflow-hidden">
                    <BaseButton
                      accessibilityLabel={q()}
                      onPress={() => submit(q())}
                      class={`flex-row items-center gap-1.5 pl-3 pr-1.5 py-1.5 active:bg-muted ${focus}`}
                    >
                      <Icon name="clock" class="size-3.5" color="muted-foreground" />
                      <Text class="text-sm text-foreground">{q()}</Text>
                    </BaseButton>
                    <BaseButton
                      accessibilityLabel={`删除搜索 ${q()}`}
                      onPress={() => props.core.history.remove("searches", q())}
                      class={`items-center justify-center pl-1 pr-2.5 py-1.5 active:bg-muted ${focus}`}
                    >
                      <Icon name="close" class="size-3" color="muted-foreground" />
                    </BaseButton>
                  </View>
                )}
              </Each>
            </Scroll>
          </If>

          <If when={!histories().length}>
            <View class="flex-1 items-center justify-center gap-1.5 pb-16">
              <View class="size-10 items-center justify-center rounded-full bg-muted mb-1">
                <Icon name="search" small color="muted-foreground" />
              </View>
              <Text class="text-sm font-medium text-foreground">还没有搜索记录</Text>
              <Text class="text-xs text-muted-foreground">输入歌曲、歌手、专辑或歌单名称开始搜索</Text>
            </View>
          </If>
        </View>
      </If>

      {/* Active search results workspace */}
      <If when={available().length > 0 && !!query().trim() && !!source()}>
        <If when={first.loading() && !results().length}>
          <View class="flex-1 min-h-0 items-center justify-center gap-3 pb-16">
            <Spinner label="正在搜索…" />
            <Text class="text-sm text-muted-foreground">正在搜索…</Text>
            <Button label="取消" size="sm" variant="secondary" onPress={cancelSearch} />
          </View>
        </If>

        <If when={!!first.error() || !!error()}>
          <View class="pl-(--content-inset-left) pr-(--content-inset-right) pt-1">
            <View class="rounded-(--radius-panel) border border-border bg-card p-4 gap-3">
              <Text class="text-sm text-destructive">{error() || String(first.error())}</Text>
              <View class="flex-row gap-2">
                <Button
                  label="重试搜索"
                  size="sm"
                  onPress={() => {
                    cancelled.set(false);
                    error.set("");
                    first.refetch();
                  }}
                />
                <Button label="去设置…" size="sm" variant="ghost" onPress={props.settings} />
              </View>
            </View>
          </View>
        </If>

        <If when={!first.loading() && !first.error() && !error() && !results().length}>
          <View class="flex-1 min-h-0 items-center justify-center gap-1.5 px-6 pb-16">
            <View class="size-10 items-center justify-center rounded-full bg-muted mb-1">
              <Icon name="search" small color="muted-foreground" />
            </View>
            <Text class="text-sm font-medium text-foreground">
              {cancelled() ? "搜索已取消" : "没有找到相关结果"}
            </Text>
            <Text class="text-xs text-muted-foreground">
              {cancelled() ? "在顶部搜索框重新搜索" : "试试其他关键词，或在右上角切换音乐来源"}
            </Text>
          </View>
        </If>

        {/* Full-height detail track table: artwork, inline likes, fixed heading + virtual scroll */}
        <If when={results().length > 0 && kind() === "track"}>
          <SourceTracks
            core={props.core}
            tracks={results()}
            active={props.active && !confirm()}
            detail
            fixedHeading
            footer={paginationFooter()}
            onLoadMore={() => wantMore.set(true)}
          />
        </If>

        {/* Full-height virtual entity list aligned to the exact same 58pt rhythm as tracks */}
        <If when={results().length > 0 && kind() !== "track"}>
          <View class="flex-1 min-h-0">
            <View class="shrink-0 pl-(--content-inset-left) pr-(--content-inset-right)">
              <View class="flex-row items-center gap-3 h-(--detail-heading-height) px-4 border-b border-border">
                <Text class="w-10 shrink-0 text-xs text-muted-foreground">#</Text>
                <Text class="flex-1 min-w-0 pr-6 text-xs text-muted-foreground" style={{ flexGrow: 1.6 }}>
                  {entityColumnHeading(kind())}
                </Text>
                <Text class="flex-1 min-w-0 pr-6 text-xs text-muted-foreground">信息</Text>
                <View class="w-6 shrink-0" />
              </View>
            </View>
            <List
              class="flex-1 min-h-0"
              each={results()}
              keyExtractor={(entity, index) =>
                entity.reference ? JSON.stringify(entity.reference) : `${kind()}:${index}:${entity.title}`
              }
              estimatedItemSize={token("detail-row-height")}
              loadMoreThreshold={1}
              onLoadMore={() => wantMore.set(true)}
              contentStyle={{ paddingTop: 10, paddingBottom: token("dock-scroll-clearance") }}
              footer={
                <View class="pl-(--content-inset-left) pr-(--content-inset-right)">
                  {paginationFooter()}
                </View>
              }
            >
            {(entity, index) => {
              const isArtist = () => kind() === "artist";
              const subtitle = () => entitySubtitle(entity(), kind());
              const meta = () => entityMetadata(entity());
              return (
                <View class="pl-(--content-inset-left) pr-(--content-inset-right)">
                  <BaseButton
                    accessibilityLabel={entity().title}
                    disabled={!entity().reference}
                    onPress={() => openEntity(entityRoute(entity()))}
                    class={`flex-row items-center gap-3 px-4 h-(--detail-row-height) rounded-(--radius) active:bg-muted disabled:opacity-40 ${focus}`}
                  >
                    <Text
                      class="w-10 shrink-0 text-xs text-muted-foreground"
                      style={{ fontVariant: ["tabular-nums"] }}
                    >
                      {String(index() + 1).padStart(2, "0")}
                    </Text>
                    <View
                      class="flex-1 min-w-0 flex-row items-center gap-2.5 pr-6"
                      style={{ flexGrow: 1.6 }}
                    >
                      <View
                        class={`size-9 shrink-0 overflow-hidden bg-muted items-center justify-center ${
                          isArtist() ? "rounded-full" : "rounded-(--radius-inset)"
                        }`}
                      >
                        <If
                          when={!!entity().cover}
                          else={<Icon name={entityFallbackIcon(kind())} small color="muted-foreground" />}
                        >
                          <Image src={entity().cover} class="size-9" style={{ objectFit: "cover" }} />
                        </If>
                      </View>
                      <View class="flex-1 min-w-0 gap-0.5">
                        <Text
                          class="text-xs font-medium text-foreground"
                          style={{ numberOfLines: 1, textOverflow: "ellipsis" }}
                        >
                          {entity().title}
                        </Text>
                        <If when={!!subtitle()}>
                          <Text
                            class="text-(--detail-artist-size) text-muted-foreground"
                            style={{ numberOfLines: 1, textOverflow: "ellipsis" }}
                          >
                            {subtitle()}
                          </Text>
                        </If>
                      </View>
                    </View>
                    <Text
                      class="flex-1 min-w-0 pr-6 text-xs text-muted-foreground"
                      style={{ numberOfLines: 1, textOverflow: "ellipsis", fontVariant: ["tabular-nums"] }}
                    >
                      {meta() || "—"}
                    </Text>
                    <View class="w-6 shrink-0 items-end opacity-40">
                      <Icon name="right" class="size-3.5" color="muted-foreground" />
                    </View>
                  </BaseButton>
                </View>
              );
            }}
            </List>
          </View>
        </If>
      </If>

      <Dialog
        open={confirm()}
        onOpenChange={(e) => confirm.set(e.open)}
        class="w-96 max-w-full self-center mt-24 p-5 gap-4"
      >
        <Text class="text-lg font-semibold text-foreground">清空搜索历史？</Text>
        <Text class="text-sm text-muted-foreground">删除所有搜索记录。</Text>
        <View class="flex-row justify-end gap-2">
          <Button label="取消" onPress={() => confirm.set(false)} />
          <Button
            label="清空"
            variant="destructive"
            onPress={() => {
              props.core.history.clear("searches");
              confirm.set(false);
            }}
          />
        </View>
      </Dialog>
    </View>
  );
}
