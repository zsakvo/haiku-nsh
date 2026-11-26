import { computed, effect, Image, List, platform, Scroll, signal, Text, token, View, type ListRef, type ViewRef } from "nsh";
import { Button as BaseButton, Select as BaseSelect } from "nsh/base";
import { isAvailable as desktopAvailable, Menu as DesktopMenu } from "nsh/desktop";
import { focus } from "../ui/Button";
import { NavigationScrollHeader } from "../ui/Chrome";
import { CategoryChip, ErrorState, Spinner } from "../ui/Discovery";
import { Icon } from "../ui/Icon";
import { popupItem, popupSurface } from "../ui/Surfaces";

export type CategoryPlaylist = {
  reference?: unknown;
  title: string;
  cover?: string | null;
  creator?: string | null;
  description?: string | null;
  track_count?: number | null;
  trackCount?: number | null;
};

export type CategoryItem = { id: string; title: string };
export type CategorySort = { id: string; title: string };

const gridGap = 20;
const minCardWidth = 160;

function trackCountOf(item: CategoryPlaylist): number | null {
  const count = item.track_count ?? item.trackCount;
  return typeof count === "number" && count > 0 ? count : null;
}

function playlistMeta(item: CategoryPlaylist): string {
  const parts: string[] = [];
  if (item.creator?.trim()) parts.push(item.creator.trim());
  const count = trackCountOf(item);
  if (count !== null) parts.push(`${count} 首`);
  return parts.join(" · ");
}

function SortSelect(props: {
  value: string;
  sorts: CategorySort[];
  onChange: (value: string) => void;
}) {
  const currentTitle = () => props.sorts.find((s) => s.id === props.value)?.title ?? props.sorts[0]?.title ?? "排序";
  const nativeMenu = platform.os === "macos" && desktopAvailable;
  const triggerClass = `min-h-7 flex-row items-center gap-1.5 rounded-full bg-category-chip px-3 py-1 active:bg-category-chip-active selected:bg-category-chip-active ${focus}`;
  const triggerBody = () => (
    <>
      <Text class="text-xs text-category-chip-foreground">{currentTitle()}</Text>
      <Icon name="down" class="size-3.5" color="muted-foreground" />
    </>
  );

  if (nativeMenu) {
    const open = signal(false);
    let anchor: ViewRef | undefined;
    async function openMenu() {
      if (open() || !props.sorts.length) return;
      open.set(true);
      try {
        const measured = await anchor?.measure();
        const at = measured
          ? { x: measured.pagePosition.x, y: measured.pagePosition.y + measured.bounds.size.height + 4 }
          : undefined;
        await DesktopMenu.popup(
          props.sorts.map((s) => ({
            label: s.title,
            checked: s.id === props.value,
            onSelect: () => {
              if (s.id !== props.value) props.onChange(s.id);
            },
          })),
          at,
        );
      } finally {
        open.set(false);
      }
    }
    return (
      <View
        ref={(node) => {
          anchor = node;
        }}
        class="shrink-0"
      >
        <BaseButton accessibilityLabel="排序" selected={open()} onPress={() => void openMenu()} class={triggerClass}>
          {triggerBody()}
        </BaseButton>
      </View>
    );
  }

  return (
    <BaseSelect
      accessibilityLabel="排序"
      value={props.value}
      items={props.sorts.map((s) => ({ value: s.id, title: s.title }))}
      align="end"
      popupClass={`${popupSurface} min-w-32 p-1`}
      listClass="max-h-64"
      itemClass={`${popupItem} selected:bg-accent`}
      onChange={(e) => {
        if (e.value !== null) props.onChange(e.value);
      }}
      trigger={<View class={triggerClass}>{triggerBody()}</View>}
    />
  );
}

function PlaylistCard(props: { item: CategoryPlaylist; onPress: () => void }) {
  const meta = () => playlistMeta(props.item);
  const fallbackDesc = () => (!meta() ? props.item.description?.trim() || "" : "");
  return (
    <BaseButton
      accessibilityLabel={props.item.title}
      disabled={!props.item.reference}
      onPress={props.onPress}
      class={`group gap-2 items-stretch rounded-(--radius-panel) active:opacity-80 disabled:opacity-40 ${focus}`}
    >
      <View class="w-full aspect-square overflow-hidden rounded-(--radius) border border-category-cover-border bg-muted items-center justify-center">
        <If when={!!props.item.cover}>
          <Image src={props.item.cover!} class="absolute inset-0" style={{ objectFit: "cover" }} />
        </If>
        <If when={!props.item.cover}>
          <Icon name="music" color="muted-foreground" />
        </If>
      </View>
      <View class="gap-1 px-0.5">
        <Text
          class="w-full font-medium text-foreground"
          style={{ fontSize: token("sidebar-text-size"), numberOfLines: 2, textOverflow: "ellipsis" }}
        >
          {props.item.title}
        </Text>
        <If when={!!meta()}>
          <Text class="w-full text-xs text-muted-foreground" style={{ numberOfLines: 1, textOverflow: "ellipsis" }}>
            {meta()}
          </Text>
        </If>
        <If when={!!fallbackDesc()}>
          <Text class="w-full text-xs text-muted-foreground" style={{ numberOfLines: 1, textOverflow: "ellipsis" }}>
            {fallbackDesc()}
          </Text>
        </If>
      </View>
    </BaseButton>
  );
}

/** Editorial category detail surface: title + sibling switch + native sort menu + virtualized gallery. */
export function CategoryDetail(props: {
  categoryId: string;
  title: string;
  siblings: CategoryItem[];
  sourceName?: string;
  sorts: CategorySort[];
  sort: string;
  onSort: (id: string) => void;
  onSelectCategory: (id: string, title: string) => void;
  items: CategoryPlaylist[];
  total: number | null;
  loading: boolean;
  error: string;
  onRetry: () => void;
  hasMore: boolean;
  busy: boolean;
  moreError: string;
  onMore: () => void;
  onOpen: (item: CategoryPlaylist) => void;
  onScrolled: (scrolled: boolean) => void;
}) {
  const width = signal(0);
  let list: ListRef | undefined;

  effect(() => {
    props.categoryId;
    props.sort;
    list?.scrollToOffset({ offset: 0, animated: false });
    props.onScrolled(false);
  });

  const columns = () =>
    width() > 0 ? Math.max(2, Math.floor((width() + gridGap) / (minCardWidth + gridGap))) : 4;

  type GridRow = { id: string; items: CategoryPlaylist[] };
  const galleryRows = computed<GridRow[]>(() => {
    const all = props.items;
    const cols = columns();
    const out: GridRow[] = [];
    for (let i = 0; i < all.length; i += cols) {
      const slice = all.slice(i, i + cols);
      out.push({
        id: `${cols}:${slice.map((item, idx) => (item.reference ? JSON.stringify(item.reference) : `${i + idx}`)).join("|")}`,
        items: slice,
      });
    }
    return out;
  });

  const rowEstimate = () => {
    if (width() <= 0) return 244;
    const cols = columns();
    const cardWidth = (width() - gridGap * (cols - 1)) / cols;
    return Math.round(cardWidth) + 84;
  };

  return (
    <List
      ref={(node) => {
        list = node;
      }}
      testID="categories-scroll"
      class="flex-1"
      each={galleryRows()}
      keyExtractor={(row) => row.id}
      estimatedItemSize={rowEstimate()}
      onScroll={(e) => props.onScrolled(e.offset.y > 0)}
      onLayout={(e) =>
        width.set(Math.max(0, e.bounds.size.width - token("content-inset-left") - token("content-inset-right")))
      }
      contentClass="pl-(--content-inset-left) pr-(--content-inset-right) pb-(--dock-scroll-clearance)"
      header={
        <NavigationScrollHeader class="gap-5 pb-6">
          <View class="gap-3">
            <View class="flex-row items-end justify-between gap-4">
              <View class="flex-1 min-w-0 gap-1.5">
                <Text testID="category-title" role="header" class="text-2xl font-semibold text-foreground">
                  {props.title}
                </Text>
                <View class="flex-row flex-wrap items-center gap-x-2 gap-y-0.5">
                  <If when={!props.loading}>
                    <Text class="text-xs text-muted-foreground">{`已显示 ${props.items.length} 个歌单`}</Text>
                  </If>
                  <If when={!props.loading && props.total != null && props.total > props.items.length}>
                    <Text class="text-xs text-muted-foreground opacity-50">·</Text>
                    <Text class="text-xs text-muted-foreground">{`共 ${props.total} 个`}</Text>
                  </If>
                  <If when={!!props.sourceName}>
                    <If when={!props.loading}>
                      <Text class="text-xs text-muted-foreground opacity-50">·</Text>
                    </If>
                    <Text class="text-xs text-muted-foreground">{props.sourceName}</Text>
                  </If>
                </View>
              </View>
              <If when={props.sorts.length > 0}>
                <View class="shrink-0">
                  <SortSelect value={props.sort} sorts={props.sorts} onChange={props.onSort} />
                </View>
              </If>
            </View>

            <If when={props.siblings.length > 1}>
              <Scroll horizontal class="shrink-0" contentClass="flex-row items-center gap-2 py-0.5">
                <Each of={props.siblings} key={(c) => c.id}>
                  {(c) => (
                    <CategoryChip
                      label={c().title}
                      selected={c().id === props.categoryId}
                      onPress={() => {
                        if (c().id !== props.categoryId) props.onSelectCategory(c().id, c().title);
                      }}
                    />
                  )}
                </Each>
              </Scroll>
            </If>
          </View>

          <View class="h-px bg-settings-divider" />

          <If when={props.loading}>
            <Spinner label="正在加载歌单…" />
          </If>
          <If when={!!props.error}>
            <ErrorState message={props.error} retry={props.onRetry} />
          </If>
          <If when={!props.loading && !props.error && !props.items.length}>
            <View class="items-center gap-2 py-12">
              <Icon name="playlist" color="muted-foreground" />
              <Text class="text-sm text-muted-foreground">此分类暂无歌单</Text>
            </View>
          </If>
        </NavigationScrollHeader>
      }
      footer={
        <View class="items-center pt-2 pb-4 gap-3">
          <If when={props.busy}>
            <Spinner label="正在加载更多歌单…" />
          </If>
          <If when={!!props.moreError}>
            <ErrorState message={props.moreError} retry={props.onMore} />
          </If>
          <If when={props.hasMore && !props.busy}>
            <BaseButton
              onPress={props.onMore}
              class={`items-center justify-center rounded-full min-h-8 px-5 py-1.5 bg-category-chip active:bg-category-chip-active ${focus}`}
            >
              <Text class="text-xs font-medium text-category-chip-foreground">加载更多歌单</Text>
            </BaseButton>
          </If>
          <If when={!props.hasMore && !props.loading && props.items.length > 0}>
            <Text class="text-xs text-muted-foreground opacity-60">{`已展示全部 ${props.items.length} 个歌单`}</Text>
          </If>
        </View>
      }
    >
      {(row) => (
        <View class="flex-row items-start pb-6" style={{ gap: gridGap }}>
          <Each of={Array.from({ length: columns() }, (_, index) => index)}>
            {(col) => (
              <View class="flex-1 min-w-0">
                <If when={!!row().items[col()]}>
                  <PlaylistCard item={row().items[col()]} onPress={() => props.onOpen(row().items[col()])} />
                </If>
              </View>
            )}
          </Each>
        </View>
      )}
    </List>
  );
}
