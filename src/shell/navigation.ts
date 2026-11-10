import { signal } from "nsh";
import type { IconName } from "../ui/Icon";

/** Opaque host-encoded identity. Do not split on punctuation in provider IDs. */
export type EntityRef = string & { readonly __entityRef: unique symbol };
export const destinations = [
  { id: "home", label: "推荐", icon: "home", group: "发现", title: "让音乐在这里相遇", description: "推荐内容将在来源插件接入后提供。" },
  { id: "categories", label: "分类", icon: "grid", group: "发现", title: "发现不同的音乐", description: "分类浏览将在来源插件接入后提供。" },
  { id: "radio", label: "电台", icon: "radio", group: "发现", title: "听听下一首", description: "电台将在来源插件接入后提供。" },
  { id: "search", label: "搜索", icon: "search", group: "发现", title: "搜索", description: "搜索音乐、歌手或节目" },
  { id: "library", label: "本地曲库", icon: "music", group: "音乐库", title: "你的音乐，放在这里", description: "添加音乐文件夹，扫描歌曲、专辑与艺术家。" },
  { id: "favorites", label: "喜欢", icon: "heart", group: "音乐库", title: "喜欢", description: "点击底栏的喜欢按钮，或在曲目菜单中选择喜欢。" },
  { id: "playlists", label: "歌单", icon: "music", group: "歌单", title: "歌单", description: "把音乐放在一起。" },
  { id: "history", label: "历史", icon: "clock", group: "音乐库", title: "留下听过的旋律", description: "播放历史将在历史记录功能接入后提供。" },
  { id: "statistics", label: "统计", icon: "stats", group: "音乐库", title: "看见你的聆听习惯", description: "聆听统计将在后续版本提供。" },
  { id: "downloads", label: "下载", icon: "download", group: "工具", title: "让音乐随时相伴", description: "下载管理将在下载功能接入后提供。" },
] as const satisfies readonly { id: string; label: string; icon: IconName; group: string; title: string; description: string }[];
export type PageKind = typeof destinations[number]["id"];
export type Route = { [K in PageKind]: { kind: K } }[PageKind]
  | { kind: "source"; source: string }
  | { kind: "playlist" | "album" | "artist" | "chart" | "program"; ref: EntityRef };
export function routeKey(route: Route): string {
  return "ref" in route ? `${route.kind}:${route.ref}` : route.kind === "source" ? `source:${route.source}` : route.kind;
}
export function createNavigation(initial: Route = { kind: "home" }) {
  const history = signal<Route[]>([initial]);
  const index = signal(0);
  const current = () => history()[index()];
  function navigate(route: Route) {
    if (routeKey(route) === routeKey(current())) return;
    const next = [...history().slice(0, index() + 1), route].slice(-100);
    history.set(next);
    index.set(next.length - 1);
  }
  const canBack = () => index() > 0;
  const canForward = () => index() + 1 < history().length;
  return { current, history, index, navigate, canBack, canForward,
    back: () => { if (canBack()) index.set(index() - 1); },
    forward: () => { if (canForward()) index.set(index() + 1); },
  };
}
