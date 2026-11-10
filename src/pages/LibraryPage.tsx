import { cleanup, effect, List, resource, Scroll, signal, Text, View } from "nsh";
import { Core, LibraryService, TrackList } from "haiku/native";
import type { EntityRef, Route } from "../shell/navigation";
import { mirrorCore } from "../state/core";
import { EmptyState } from "../ui/EmptyState";
import { SearchField } from "../ui/SearchField";
import { ToggleGroup } from "../ui/LibraryControls";
import { Button } from "../ui/Button";
import { Dialog, Menu, MenuItem } from "../ui/Surfaces";
import { TrackEditor } from "./TrackEditor";

const views = [{ id: "tracks", label: "歌曲" }, { id: "albums", label: "专辑" }, { id: "artists", label: "艺术家" }];
const sorts = [{ id: "title", label: "标题" }, { id: "artist", label: "艺术家" }, { id: "album", label: "专辑" },
  { id: "track", label: "曲号" }, { id: "duration", label: "时长" }, { id: "path", label: "路径" }];
type Group = { key: EntityRef; title: string; subtitle: string; count: number; cover?: string };
type Groups = { total: number; items: Group[] };
type GroupDetail = { title: string; count: number };

/** Retained local browse page; Rust owns query results, row selection and scroll position. */
export function LibraryPage(props: {
  core: Core; active: boolean; group?: EntityRef; groupKind?: "album" | "artist";
  navigate: (route: Route) => void; injectedFolders?: string[];
}) {
  const mirrors = mirrorCore(props.core);
  const library = props.core.library;
  const view = signal("tracks");
  const search = signal("");
  const sort = signal(props.group ? "track" : "title");
  const descending = signal(false);
  const error = signal("");
  const busy = signal(false);
  const rootsOpen = signal(false);
  const errorsOpen = signal(false);
  const selected = signal<string[]>([]);
  const editor = signal<string[] | null>(null);
  const groupLimit = signal(100);
  const query = () => ({ search: search(), sort: sort(), descending: descending(), group: props.group ?? null });
  const source = resource(async () => library.query(query()));
  const detail = resource(() => ({ key: props.group, revision: mirrors.library().revision }),
    async ({ key }) => key ? await library.describeGroup(key) as GroupDetail : null);
  cleanup(() => { if (source()) void library.releaseQuery(source()!); });
  // One source per retained page. Filter updates are serialized in the engine.
  effect(() => {
    const handle = source();
    const request = query();
    if (handle) void library.setQuery(handle, request).catch((e) => error.set(String(e)));
  });
  const groups = resource(
    () => ({ kind: view(), search: search(), descending: descending(), sort: sort(),
      revision: mirrors.library().revision, limit: groupLimit() }),
    async (request): Promise<Groups> => {
      if (request.kind === "tracks" || props.group) return { total: 0, items: [] };
      const items: Group[] = [];
      let total = 0;
      for (let offset = 0; offset < request.limit; offset += 100) {
        const page = await library.groups(request.kind,
          { search: request.search, descending: request.descending, sort: request.sort }, offset, 100) as Groups;
        total = page.total; items.push(...page.items);
        if (items.length >= total) break;
      }
      return { total, items };
    },
  );
  const roots = () => { mirrors.library().revision; return library.roots(); };
  const showTracks = () => !!props.group || view() === "tracks";
  const sortChoices = () => showTracks() ? sorts : [{ id: "title", label: "名称" }];
  async function act(action: () => Promise<unknown>) {
    error.set(""); busy.set(true);
    try { await action(); } catch (e) { error.set(String(e)); } finally { busy.set(false); }
  }
  async function addRoots() {
    const paths = props.injectedFolders ?? await LibraryService.pickFolders();
    if (paths.length) { await library.addRoots(paths); await library.scan(); }
  }
  async function action(command: string, ids: string[]) {
    if (command === "edit") { editor.set(ids); return; }
    await act(async () => {
      if (command === "reveal") { for (const id of ids) await library.reveal(id); return; }
      const tracks = ids.map((id) => library.track(id)).filter(Boolean);
      if (command === "append") await props.core.player.append(tracks);
      if (command === "next") for (const track of [...tracks].reverse()) await props.core.player.playNext(track);
    });
  }
  return <View testID="page-library" class="flex-1 min-h-0"
    style={{ display: props.active ? "flex" : "none", flexDirection: "column" }}>
    <View class="gap-3 px-(--content-inset) pb-4">
      <View class="flex-row items-center gap-2">
        <Text role="header" class="flex-1 text-xl font-semibold text-foreground">
          {props.group ? (detail()?.title ?? (props.groupKind === "album" ? "本地专辑" : "本地艺术家")) : "本地曲库"}
        </Text>
        <Button label="管理文件夹…" size="sm" disabled={busy()} onPress={() => rootsOpen.set(true)} />
        <Button label="重新扫描" size="sm" disabled={busy() || mirrors.library().scanning || !roots().length}
          onPress={() => void act(() => library.scan())} />
      </View>
        <View class="flex-row items-center gap-3 flex-wrap">
          <If when={!props.group}><ToggleGroup items={views} value={view()} onChange={(id) => view.set(id)} /></If>
          <View class="flex-1 min-w-48"><SearchField value={search()} label="筛选本地曲库"
            placeholder="标题、艺术家、专辑或路径" onChange={(value) => search.set(value)} /></View>
          <Menu trigger={<Button label={`排序：${showTracks() ? sorts.find((s) => s.id === sort())?.label : "名称"}`} size="sm" onPress={() => {}} />}>
            {sortChoices().map((s) => <MenuItem checked={!showTracks() || sort() === s.id} onSelect={() => sort.set(s.id)}>
              <Text>{s.label}</Text>
            </MenuItem>)}
            <MenuItem checked={descending()} onSelect={() => descending.set(!descending())}><Text>降序</Text></MenuItem>
          </Menu>
        </View>
      <If when={mirrors.library().scanning}>
        <View class="flex-row items-center gap-2">
          <Text class="flex-1 text-sm text-muted-foreground">
            正在扫描 {mirrors.library().progress.scanned} / {mirrors.library().progress.total} · {mirrors.library().progress.current}
          </Text>
          <Button label="取消扫描" size="sm" onPress={() => library.cancel()} />
        </View>
      </If>
      <If when={!mirrors.library().scanning && (!!mirrors.library().report.skipped || mirrors.library().report.cancelled || !!mirrors.library().report.errors.length)}>
        <View class="gap-2">
          <View class="flex-row items-center gap-2">
            <Text class="text-xs text-muted-foreground">{mirrors.library().report.cancelled ? "已取消；保留上次完整索引" : `扫描完成 · ${mirrors.library().report.count} 首 · 跳过 ${mirrors.library().report.skipped} 个文件`}</Text>
            <If when={!!mirrors.library().report.errors.length}><Button label={errorsOpen() ? "收起错误" : "查看扫描错误"}
              size="sm" variant="ghost" onPress={() => errorsOpen.set(!errorsOpen())} /></If>
          </View>
          <If when={errorsOpen()}><Scroll class="max-h-24" contentClass="gap-1">
            {mirrors.library().report.errors.map((message) => <Text class="text-xs text-destructive">{message}</Text>)}
          </Scroll></If>
        </View>
      </If>
      <If when={!!error() || !!source.error() || !!groups.error()}>
        <View class="gap-2"><Text class="text-sm text-destructive">{error() || String(source.error() ?? groups.error())}</Text>
          <Button label="重试" size="sm" onPress={() => {
            error.set("");
            if (source()) void act(() => library.setQuery(source()!, query()));
            else source.refetch();
            groups.refetch();
          }} />
        </View>
      </If>
      <If when={showTracks() && !!selected().length}>
        <View class="flex-row gap-2 items-center">
          <Text class="text-xs text-muted-foreground">已选择 {selected().length} 首</Text>
          <Button label="编辑信息…" size="sm" onPress={() => editor.set(selected())} />
          <Button label="加入队列" size="sm" onPress={() => void action("append", selected())} />
        </View>
      </If>
    </View>
    <If when={!roots().length}>
      <EmptyState title="你的音乐，放在这里" description="添加音乐文件夹，扫描歌曲、专辑与艺术家。"
        action={{ label: "添加文件夹…", onPress: () => void act(addRoots) }} />
    </If>
    <If when={!!roots().length && !source()}>
      <Text class="p-5 text-sm text-muted-foreground">正在加载曲库…</Text>
    </If>
    <If when={!!roots().length && !!source() && showTracks()}>
      <TrackList source={source()!} active={props.active && !editor() && !rootsOpen()}
        currentId={mirrors.player()?.current?.id ?? null}
        onPlay={(event) => void act(() => library.playSource(event.source, event.id))}
        onAction={(event) => void action(event.command, event.ids)}
        onSelectionChange={(event) => selected.set(event.ids)} />
      <If when={mirrors.library().report.count === 0 && !mirrors.library().scanning}>
        <Text class="px-5 py-3 text-sm text-muted-foreground">尚未找到歌曲。请重新扫描或添加其他文件夹。</Text>
      </If>
    </If>
    <If when={!!roots().length && !showTracks()}>
      <List each={groups()?.items ?? []} keyExtractor={(group) => group.key} estimatedItemSize={64}
        class="flex-1 min-h-0" empty={<Text class="p-5 text-sm text-muted-foreground">{groups.loading() ? "正在加载分组…" : "没有符合条件的分组"}</Text>}>
        {(group) => <View class="px-(--content-inset)">
          <Button label={`${group().title} · ${group().subtitle} · ${group().count} 首`}
            variant="ghost" onPress={() => props.navigate({ kind: view() === "albums" ? "album" : "artist", ref: group().key })} />
        </View>}
      </List>
      <If when={(groups()?.items.length ?? 0) < (groups()?.total ?? 0)}>
        <Button label="加载更多分组" onPress={() => groupLimit.update((n) => n + 100)} />
      </If>
    </If>
    <Dialog open={rootsOpen()} onOpenChange={(event) => { if (!event.open) rootsOpen.set(false); }}
      class="w-128 p-5 gap-4">
      <Text role="header" class="text-lg font-semibold text-foreground">管理文件夹</Text>
      <Text class="text-xs text-muted-foreground">移除后重新扫描以更新曲库，元数据覆盖仍会保留。</Text>
      <Scroll class="max-h-80" contentClass="gap-2">
        {roots().map((root) => <View class="flex-row items-center gap-2">
          <Text class="flex-1 min-w-0 text-sm text-foreground">{root}</Text>
          <Button label="移除" size="sm" disabled={busy() || mirrors.library().scanning}
            onPress={() => void act(() => library.removeRoot(root))} />
        </View>)}
      </Scroll>
      <View class="flex-row gap-2">
        <Button label="添加文件夹…" disabled={busy() || mirrors.library().scanning} onPress={() => void act(addRoots)} />
        <Button label="完成" onPress={() => rootsOpen.set(false)} />
      </View>
    </Dialog>
    <If when={!!editor()}><TrackEditor core={props.core} ids={editor()!} onClose={() => editor.set(null)} /></If>
  </View>;
}
