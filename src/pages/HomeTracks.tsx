import { cleanup, effect, Image, List, platform, reduceMotion, signal, Text, token, View, type JSX, type ListRef } from "nsh";
import { Button as BaseButton, Submenu } from "nsh/base";
import { PluginService, type Core } from "haiku/native";
import type { Collection } from "../state/collections";
import { focus } from "../ui/Button";
import { Icon } from "../ui/Icon";
import { ContextMenu, Menu, MenuItem, popupItem, Tooltip } from "../ui/Surfaces";
import { PlaylistEditor } from "./PlaylistEditor";

export type HomeTrack = {
  source: string; id: string; title: string; cover?: string | null;
  artists: (string | { name: string })[]; quality?: string[] | null;
};
const trackKey = (track: HomeTrack) => PluginService.reference(track.source, "track", track.id);
const pageSize = 6;
const rowsPerColumn = 3;

/** Each virtual page contains two columns of three songs; the shelf owns paging and actions. */
export function HomeTracks(props: { core: Core; tracks: HomeTrack[]; active: boolean; width?: number }) {
  // The page width the parent already knows: at 0 every page is 1 pt wide until the first layout,
  // and the list binds all of them at once.
  const width = signal(props.width ?? 0), page = signal(0), hovered = signal(false);
  const error = signal(""), pending = signal(""), creating = signal<HomeTrack[] | null>(null);
  const liked = signal(new Set(props.core.collections.likedKeys() as string[]));
  const playlists = signal(props.core.collections.list() as Collection[]);
  let scroll: ListRef | undefined;
  cleanup(props.core.collections.onLiked(keys => liked.set(new Set(keys as string[]))));
  cleanup(props.core.collections.onChange(() => playlists.set(props.core.collections.list() as Collection[])));
  const pages = () => Array.from({ length: Math.ceil(props.tracks.length / pageSize) },
    (_, index) => props.tracks.slice(index * pageSize, (index + 1) * pageSize));
  const lastPage = () => Math.max(0, pages().length - 1);
  const height = () => token("home-track-row-height") * rowsPerColumn;
  effect(() => {
    props.tracks;
    page.set(0);
    scroll?.scrollToOffset({ offset: 0, animated: false });
  });
  function move(delta: number) {
    const next = Math.max(0, Math.min(lastPage(), page() + delta));
    page.set(next);
    scroll?.scrollToOffset({ offset: next * width(), animated: !reduceMotion() });
  }
  function resize(next: number) {
    if (next <= 0 || next === width()) return;
    width.set(next);
    scroll?.scrollToOffset({ offset: page() * next, animated: false });
  }
  async function run(work: () => unknown | Promise<unknown>) {
    error.set("");
    try { await work(); } catch (e) { error.set(String(e)); }
  }
  async function play(track: HomeTrack) {
    if (pending()) return;
    pending.set(trackKey(track));
    await run(() => props.core.player.play(props.tracks,
      props.tracks.findIndex(candidate => trackKey(candidate) === trackKey(track))));
    pending.set("");
  }
  function commands(track: HomeTrack) {
    return <>
      <MenuItem onSelect={() => void play(track)}><Text>播放</Text></MenuItem>
      <MenuItem onSelect={() => void run(() => props.core.player.playNext(track))}><Text>下一首播放</Text></MenuItem>
      <MenuItem onSelect={() => void run(() => props.core.player.append([track]))}><Text>加入队列</Text></MenuItem>
      <MenuItem onSelect={() => void run(() => props.core.downloads.enqueue([track]))}><Text>下载</Text></MenuItem>
      <MenuItem onSelect={() => void run(() => props.core.collections.setLiked(track))}>
        <Text>{liked().has(trackKey(track)) ? "取消喜欢" : "喜欢"}</Text>
      </MenuItem>
      <Submenu trigger={<View class={`${popupItem} flex-row items-center justify-between gap-4`}>
        <Text>加入歌单</Text><Icon name="right" small color="muted-foreground" />
      </View>}>
        <Each of={playlists().filter(item => item.kind === "user")} key={item => item.id}>{item =>
          <MenuItem onSelect={() => void run(() => props.core.collections.add(item().id, [track]))}>
            <Text>{item().title}</Text>
          </MenuItem>}
        </Each>
        <MenuItem onSelect={() => creating.set([track])}><Text>新建歌单…</Text></MenuItem>
      </Submenu>
    </>;
  }
  const arrowVisibility = () => hovered() || !platform.desktop ? "opacity-100" : "opacity-0";
  return <View class="shrink-0 gap-2">
    {/* Expand only the interaction region into the page gutters; songs keep the shared content edge. */}
    <View testID="home-tracks" class="relative shrink-0"
      style={{ height: height(), marginLeft: -token("content-inset-left"), marginRight: -token("content-inset-right") }}
      onMouseEnter={() => hovered.set(true)} onMouseLeave={() => hovered.set(false)}>
      <List ref={node => { scroll = node; }} testID="home-tracks-scroll" horizontal pagingEnabled
        bounces={false} scrollbar={{ horizontal: { class: "hidden pointer-events-none", thumbClass: "hidden" } }}
        class="flex-1 min-w-0 ml-(--content-inset-left) mr-(--content-inset-right)"
        each={pages()} keyExtractor={items => items.map(trackKey).join(",")}
        estimatedItemSize={Math.max(1, width())}
        onLayout={event => resize(event.bounds.size.width)}
        onScrollEnd={event => {
          if (width()) page.set(Math.max(0, Math.min(lastPage(), Math.round(event.offset.x / width()))));
        }}>
        {items => <View class="flex-row shrink-0 gap-6"
          style={{ width: Math.max(1, width()), height: height() }}>
          <Each of={[0, 1]}>{column => <View class="flex-1 min-w-0">
            <Each of={items().slice(column() * rowsPerColumn, (column() + 1) * rowsPerColumn)} key={trackKey}>
              {track => <HomeTrackCard track={track()} liked={liked().has(trackKey(track()))}
                busy={pending() === trackKey(track())} disabled={!props.active || !!pending()}
                play={() => void play(track())}
                like={() => void run(() => props.core.collections.setLiked(track()))}
                download={() => void run(() => props.core.downloads.enqueue([track()]))}
                commands={() => commands(track())} />}
            </Each>
          </View>}</Each>
        </View>}
      </List>
      <If when={pages().length > 1}>
        <View class={`absolute left-1 top-0 bottom-0 w-4 items-center justify-center focus-within:opacity-100 ${arrowVisibility()}`}>
          <BaseButton accessibilityLabel="上一组歌曲" testID="home-tracks-previous" disabled={page() === 0}
            onPress={() => move(-1)} tooltip={<Tooltip label="上一组歌曲" />}
            class={`h-12 w-full items-center justify-center rounded-(--radius) hover:bg-accent active:bg-muted disabled:opacity-25 ${focus}`}>
            <Icon name="left" class="size-6 shrink-0" color="muted-foreground" />
          </BaseButton>
        </View>
        <View class={`absolute right-5 top-0 bottom-0 w-5 items-center justify-center focus-within:opacity-100 ${arrowVisibility()}`}>
          <BaseButton accessibilityLabel="下一组歌曲" testID="home-tracks-next" disabled={page() === lastPage()}
            onPress={() => move(1)} tooltip={<Tooltip label="下一组歌曲" />}
            class={`h-12 w-full items-center justify-center rounded-(--radius) hover:bg-accent active:bg-muted disabled:opacity-25 ${focus}`}>
            <Icon name="right" class="size-6 shrink-0" color="muted-foreground" />
          </BaseButton>
        </View>
      </If>
    </View>
    <If when={!!error()}><Text class="text-sm text-destructive">{error()}</Text></If>
    <If when={!!creating()}><PlaylistEditor core={props.core} tracks={creating()!} onClose={() => creating.set(null)} /></If>
  </View>;
}

function HomeTrackCard(props: {
  track: HomeTrack; liked: boolean; busy: boolean; disabled: boolean;
  play: () => void; like: () => void; download: () => void;
  commands: () => JSX.Element;
}) {
  const artists = () => props.track.artists.map(artist => typeof artist === "string" ? artist : artist.name).join(" / ") || "未知艺术家";
  const quality = () => props.track.quality?.includes("hires") ? "Hi-Res"
    : props.track.quality?.includes("lossless") ? "无损" : "";
  const visibility = platform.desktop ? "opacity-0 group-hover:opacity-100 group-focus-within:opacity-100" : "opacity-100";
  return <ContextMenu trigger={
    <View class="group flex-row items-center gap-1 rounded-(--radius-panel) hover:bg-card focus-within:bg-card"
      style={{ height: token("home-track-row-height") }}>
      <BaseButton accessibilityLabel={props.track.title} disabled={props.disabled} onPress={props.play}
        class={`flex-1 min-w-0 flex-row items-center gap-3 h-full rounded-(--radius) ${focus}`}>
        <View class="relative size-(--home-track-cover-size) shrink-0 rounded-(--radius-inset) bg-muted overflow-hidden items-center justify-center">
          <If when={!!props.track.cover}><Image src={props.track.cover!}
            class="absolute inset-0" style={{ objectFit: "cover" }} /></If>
          <If when={!props.track.cover}><Icon name="music" color="muted-foreground" /></If>
          <View class={`absolute inset-0 items-center justify-center bg-cover-shade ${visibility}`}>
            <Icon name={props.busy ? "sync" : "play"} color="cover-foreground" />
          </View>
        </View>
        <View class="flex-1 min-w-0 gap-1 items-start">
          <Text class="w-full text-sm font-semibold text-foreground"
            style={{ numberOfLines: 1, textOverflow: "ellipsis" }}>{props.track.title}</Text>
          <View class="w-full flex-row items-center gap-1">
            <If when={props.liked}><Text accessibilityLabel="已喜欢" class="shrink-0 text-sm text-primary">♥</Text></If>
            <If when={!!quality()}><View class="shrink-0 rounded-sm border border-warning px-1">
              <Text class="text-xs text-warning">{quality()}</Text>
            </View></If>
            <Text class="flex-1 min-w-0 text-xs text-muted-foreground"
              style={{ numberOfLines: 1, textOverflow: "ellipsis" }}>{artists()}</Text>
          </View>
        </View>
      </BaseButton>
      <View class={`flex-row items-center ${visibility}`}>
        <BaseButton accessibilityLabel={`下载${props.track.title}`} onPress={props.download}
          tooltip={<Tooltip label="下载" />}
          class={`size-7 rounded-(--radius) items-center justify-center hover:bg-accent active:bg-muted ${focus}`}>
          <Icon name="download" small color="muted-foreground" />
        </BaseButton>
        <BaseButton accessibilityLabel={`${props.liked ? "取消喜欢" : "喜欢"}${props.track.title}`} onPress={props.like}
          tooltip={<Tooltip label={props.liked ? "取消喜欢" : "喜欢"} />}
          class={`size-7 rounded-(--radius) items-center justify-center hover:bg-accent active:bg-muted ${focus}`}>
          <If when={props.liked}><Text class="text-base text-primary">♥</Text></If>
          <If when={!props.liked}><Icon name="heart" small color="muted-foreground" /></If>
        </BaseButton>
        <Menu trigger={<BaseButton accessibilityLabel={`更多${props.track.title}的操作`} onPress={() => {}}
          tooltip={<Tooltip label="更多操作" />}
          class={`size-7 rounded-(--radius) items-center justify-center hover:bg-accent active:bg-muted ${focus}`}>
          <Text class="text-base text-muted-foreground">…</Text>
        </BaseButton>}>{props.commands()}</Menu>
      </View>
    </View>
  }>{props.commands()}</ContextMenu>;
}
