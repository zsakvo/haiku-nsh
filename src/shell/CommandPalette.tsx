import { cleanup, computed, effect, Image, signal, Text, View } from "nsh";
import { Button, Command, type CommandEntry, type CommandItem, type CommandRef } from "nsh/base";
import { CommandKeyboard, type Core } from "haiku/native";
import { Dialog } from "../ui/Surfaces";
import { Kbd } from "../ui/Navigation";
import { Icon, type IconName } from "../ui/Icon";
import { entityRoute } from "../pages/SourceContent";
import type { Route } from "./navigation";
import type { SearchPreview, SearchRequest } from "../state/search";

export interface PaletteCommand extends CommandItem {
  icon?: IconName;
  /** Quiet trailing metadata (e.g. current route tag or active track name). */
  meta?: string;
  /** Contextual Return-key verb shown in the palette footer. */
  actionLabel?: string;
}

interface RowMeta {
  title: string;
  subtitle?: string;
  icon?: IconName;
  cover?: string;
  mediaKind?: "track" | "playlist";
  badge?: string;
  actionLabel?: string;
  error?: boolean;
}

function formatDuration(seconds: unknown): string | undefined {
  if (typeof seconds !== "number" || !Number.isFinite(seconds) || seconds <= 0) return undefined;
  const total = Math.round(seconds);
  const mins = Math.floor(total / 60);
  const secs = total % 60;
  return `${mins}:${secs.toString().padStart(2, "0")}`;
}

function isSubsequence(needle: string, haystack: string): boolean {
  let i = 0;
  for (let j = 0; j < haystack.length && i < needle.length; j++) {
    if (haystack[j] === needle[i]) i++;
  }
  return i === needle.length;
}

/** Returns match rank (higher is better) or 0 when the command does not match. */
function matchScore(item: PaletteCommand, rawQuery: string): number {
  const q = rawQuery.trim().toLowerCase();
  if (!q) return 1;
  const label = item.label.toLowerCase();
  const group = (item.group ?? "").toLowerCase();
  const keywords = (item.keywords ?? []).map((k) => k.toLowerCase());
  const tokens = q.split(/\s+/).filter(Boolean);
  let score = 0;
  for (const token of tokens) {
    if (label === token) {
      score += 100;
    } else if (label.startsWith(token)) {
      score += 60;
    } else if (label.includes(token)) {
      score += 40;
    } else if (keywords.some((k) => k.startsWith(token))) {
      score += 30;
    } else if (keywords.some((k) => k.includes(token))) {
      score += 20;
    } else if (group.includes(token)) {
      score += 10;
    } else if (/^[a-z0-9]+$/.test(token) && token.length >= 2 && (isSubsequence(token, label) || keywords.some((k) => isSubsequence(token, k)))) {
      score += 5;
    } else {
      return 0;
    }
  }
  return score;
}

export function CommandPalette(props: {
  open: boolean;
  items: PaletteCommand[];
  core?: Core;
  initialQuery?: string;
  navigate: (route: Route) => void;
  search: (request: SearchRequest) => void;
  onClose: () => void;
  onSelect: (id: string) => void;
}) {
  let commandRef: CommandRef | undefined;
  const query = signal(props.initialQuery ?? "");
  const groups = signal<SearchPreview[]>([]);
  const recentSearches = signal<string[]>([]);
  const loading = signal(false);
  const error = signal("");
  const highlighted = signal<string | null>(null);
  let epoch = 0;

  cleanup(() => { epoch++; });

  effect(() => {
    if (props.open) {
      query.set(props.initialQuery ?? "");
      highlighted.set(null);
      const searches = (props.core?.history.list() as { searches?: string[] } | undefined)?.searches ?? [];
      recentSearches.set(searches.slice(0, 3));
    }
  });

  effect(() => {
    const raw = query().trim();
    const commandOnly = raw.startsWith(">");
    const q = commandOnly ? "" : raw;
    const core = props.core;
    const open = props.open;
    const version = ++epoch;
    groups.set([]);
    error.set("");
    loading.set(!!q && open && !!core);
    if (!q || !open || !core) {
      loading.set(false);
      return;
    }
    const timer = setTimeout(() => {
      void core.sources.searchPreview(q).then((value) => {
        if (version === epoch && props.open && query().trim() === q) {
          groups.set(value as SearchPreview[]);
        }
      }).catch((e) => {
        if (version === epoch && props.open) error.set(String(e));
      }).finally(() => {
        if (version === epoch) loading.set(false);
      });
    }, 280);
    return () => clearTimeout(timer);
  });

  const resolved = computed(() => {
    const raw = query().trim();
    const commandOnly = raw.startsWith(">");
    const filterText = commandOnly ? raw.slice(1).trim() : raw;
    const meta = new Map<string, RowMeta>();

    const scored = props.items
      .map((item, index) => ({ item, index, score: matchScore(item, filterText) }))
      .filter((entry) => entry.score > 0);
    if (filterText) {
      scored.sort((a, b) => b.score - a.score || a.index - b.index);
    }

    const list: CommandItem[] = [];
    for (const { item } of scored) {
      list.push(item);
      meta.set(item.id, {
        title: item.label,
        icon: item.icon,
        badge: item.meta,
        actionLabel: item.actionLabel ?? "执行",
      });
    }

    if (!raw) {
      const recent = recentSearches();
      for (const term of recent) {
        const id = JSON.stringify(["history", term]);
        list.push({ id, label: term, group: "最近搜索" });
        meta.set(id, {
          title: term,
          icon: "clock",
          badge: "搜索",
          actionLabel: "填入搜索",
        });
      }
      return { items: list, meta };
    }

    if (commandOnly) {
      return { items: list, meta };
    }

    for (const g of groups()) {
      for (const p of g.playlists.slice(0, 3)) {
        const id = JSON.stringify(["playlist", g.source, p.reference.id]);
        list.push({ id, label: p.title, group: g.title });
        meta.set(id, {
          title: p.title,
          icon: "playlist",
          cover: typeof p.cover === "string" && p.cover ? p.cover : undefined,
          mediaKind: "playlist",
          badge: "歌单",
          actionLabel: "查看歌单",
        });
      }
      for (const t of g.tracks.slice(0, 3)) {
        const id = JSON.stringify(["track", g.source, t.id]);
        const artists = (t.artists ?? [])
          .map((a: string | { name: string }) => (typeof a === "string" ? a : a.name))
          .filter(Boolean)
          .join("、");
        const label = artists ? `${t.title} · ${artists}` : t.title;
        list.push({ id, label, group: g.title });
        meta.set(id, {
          title: t.title,
          subtitle: artists || undefined,
          icon: "music",
          cover: typeof t.cover === "string" && t.cover ? t.cover : undefined,
          mediaKind: "track",
          badge: formatDuration(t.duration) ?? "曲目",
          actionLabel: "播放",
        });
      }
      for (let i = 0; i < g.errors.length; i++) {
        const message = g.errors[i];
        const id = JSON.stringify(["error", g.source, i]);
        list.push({ id, label: message, group: g.title, disabled: true });
        meta.set(id, {
          title: message,
          icon: "close",
          error: true,
        });
      }
    }

    list.push({ id: "search-all", label: "在搜索页查看全部", group: "搜索" });
    meta.set("search-all", {
      title: "在搜索页查看全部",
      icon: "search",
      badge: `“${raw}”`,
      actionLabel: "搜索全部",
    });

    return { items: list, meta };
  });

  const actionHint = () => {
    const id = highlighted();
    if (id) {
      const verb = resolved().meta.get(id)?.actionLabel;
      if (verb) return verb;
    }
    const first = resolved().items.find((item) => !item.disabled);
    return (first && resolved().meta.get(first.id)?.actionLabel) || "打开";
  };

  function clearQuery() {
    query.set("");
    commandRef?.focus();
  }

  async function choose(id: string) {
    if (id === "search-all") {
      props.search({ query: query().trim() });
      props.onClose();
      return;
    }
    if (!id.startsWith("[")) {
      props.onSelect(id);
      return;
    }
    const parsed = JSON.parse(id) as string[];
    const [kind, sourceOrTerm, key] = parsed;
    if (kind === "history") {
      query.set(sourceOrTerm);
      commandRef?.focus();
      return;
    }
    const group = groups().find((g) => g.source === sourceOrTerm);
    if (!group) return;
    if (kind === "playlist") {
      const playlist = group.playlists.find((p) => p.reference.id === key);
      if (playlist) {
        props.navigate(entityRoute(playlist));
        props.onClose();
      }
    } else if (kind === "track" && props.core) {
      const index = group.tracks.findIndex((t) => t.id === key);
      if (index < 0) return;
      try {
        props.core.history.search(query().trim());
        await props.core.player.play(group.tracks, index);
        props.onClose();
      } catch (e) {
        error.set(String(e));
      }
    }
  }

  function renderItem(item: CommandEntry) {
    const info = () => resolved().meta.get(item.id);
    const hasTrailingHint = () => !item.disabled && !item.checked && !item.shortcutLabel;
    return <View class="flex-1 min-w-0 flex-row items-center gap-2.5">
      <If when={!!info()?.mediaKind} else={
        <View class="size-6 shrink-0 items-center justify-center">
          <If when={!!info()?.icon}>
            <Icon name={info()!.icon!} small color={info()?.error ? "destructive" : "muted-foreground"} />
          </If>
        </View>
      }>
        <View class="size-6 shrink-0 items-center justify-center overflow-hidden rounded-(--radius-inset) bg-muted">
          <If when={!!info()?.cover} else={
            <Icon name={info()?.icon ?? "music"} class="size-3.5" color="muted-foreground" />
          }>
            <Image src={info()!.cover!} class="size-6" />
          </If>
        </View>
      </If>
      <View class="flex-1 min-w-0 flex-row items-center">
        <Text
          class={`flex-1 min-w-0 text-sm ${info()?.error ? "text-destructive" : "text-popover-foreground group-focus:text-selection-foreground"}`}
          style={{ numberOfLines: 1, textOverflow: "ellipsis" }}
        >
          {info()?.title ?? item.label}
          {info()?.subtitle ? <Text class="text-muted-foreground">{` · ${info()!.subtitle}`}</Text> : null}
        </Text>
      </View>
      <View class="shrink-0 flex-row items-center gap-2">
        <If when={!!info()?.badge}>
          <Text class="max-w-36 text-xs text-muted-foreground" style={{ numberOfLines: 1, textOverflow: "ellipsis", fontVariant: ["tabular-nums"] }}>
            {info()!.badge}
          </Text>
        </If>
        <If when={item.checked}>
          <Icon name="check" small color="primary" />
        </If>
        <If when={!!item.shortcutLabel}>
          <Kbd label={item.shortcutLabel!} />
        </If>
        <If when={hasTrailingHint()}>
          <Text class="text-xs font-mono text-muted-foreground opacity-0 group-focus:opacity-100">↵</Text>
        </If>
      </View>
    </View>;
  }

  return <Dialog open={props.open} class="w-144 max-w-full self-center mt-20 overflow-hidden"
    onCancel={props.onClose}
    onOpenChange={(event) => { if (!event.open) props.onClose(); }}>
    <CommandKeyboard open={props.open} onCancel={props.onClose}>
      <Command
        ref={commandRef}
        items={resolved().items}
        query={query()}
        onQueryChange={(e) => query.set(e.query)}
        onHighlightChange={(e) => highlighted.set(e.highlighted)}
        filterable={false}
        loading={loading()}
        accessibilityLabel="命令面板"
        searchAccessibilityLabel="查找命令"
        autoFocus
        placeholder="搜索歌曲、歌单或命令…"
        onCancel={props.onClose}
        onSelect={(event) => void choose(event.id)}
        searchPrefix={<View class="size-6 shrink-0 items-center justify-center"><Icon name="search" small color="muted-foreground" /></View>}
        searchSuffix={<View class="shrink-0 flex-row items-center gap-2">
          <If when={!!error()}>
            <Text class="max-w-44 text-xs text-destructive" style={{ numberOfLines: 1, textOverflow: "ellipsis" }}>{error()}</Text>
          </If>
          <If when={!error() && loading()}>
            <Text class="text-xs text-muted-foreground">搜索中…</Text>
          </If>
          <If when={query().length > 0}>
            <Button accessibilityLabel="清空搜索" onPress={clearQuery}
              class="size-6 items-center justify-center rounded-(--radius-inset) active:bg-muted">
              <Icon name="close" class="size-3.5" color="muted-foreground" />
            </Button>
          </If>
        </View>}
        searchBarClass="min-h-12 flex-row items-center gap-2.5 px-4 border-b border-border"
        searchClass="flex-1 min-w-0 py-3 text-base text-foreground placeholder:text-muted-foreground"
        listClass="max-h-96 p-1.5"
        itemClass="group min-h-9 rounded-(--radius) px-2.5 py-1.5 focus:bg-selection active:bg-muted disabled:opacity-40 text-sm text-popover-foreground"
        groupClass="px-2.5 pt-2.5 pb-1 text-xs font-medium text-muted-foreground"
        separatorClass="h-px mx-2.5 my-1.5 bg-border"
        renderItem={renderItem}
        empty={<View class="items-center justify-center gap-1.5 px-6 py-10">
          <View class="size-9 items-center justify-center rounded-full bg-muted">
            <Icon name="search" small color="muted-foreground" />
          </View>
          <Text class="text-sm font-medium text-popover-foreground">没有匹配的结果</Text>
          <Text class="text-xs text-muted-foreground">试试其他关键词，或按 Esc 关闭</Text>
        </View>}
        footer={<View class="flex-row items-center justify-between gap-3 px-4 py-2 border-t border-border">
          <View class="flex-row items-center gap-3">
            <View class="flex-row items-center gap-1.5">
              <Kbd label="↑↓" /><Text class="text-xs text-muted-foreground">移动</Text>
            </View>
            <View class="flex-row items-center gap-1.5">
              <Kbd label="↵" /><Text class="text-xs text-muted-foreground">{actionHint()}</Text>
            </View>
          </View>
          <View class="flex-row items-center gap-1.5">
            <Kbd label="Esc" /><Text class="text-xs text-muted-foreground">关闭</Text>
          </View>
        </View>}
      />
    </CommandKeyboard>
  </Dialog>;
}
