import { signal } from "nsh";
import type { IconName } from "../ui/Icon";

/** Opaque host-encoded identity. Do not split on punctuation in provider IDs. */
export type EntityRef = string & { readonly __entityRef: unique symbol };
export const destinations = [
  { id: "home", label: "推荐", icon: "home", group: "发现", title: "让音乐在这里相遇", description: "推荐内容将在来源插件接入后提供。" },
  { id: "categories", label: "分类", icon: "grid", group: "发现", title: "发现不同的音乐", description: "分类浏览将在来源插件接入后提供。" },
  { id: "search", label: "搜索", icon: "search", group: "发现", title: "搜索", description: "搜索音乐、歌手或节目" },
  { id: "favorites", label: "喜欢", icon: "heart", group: "音乐库", title: "喜欢", description: "点击底栏的喜欢按钮，或在曲目菜单中选择喜欢。" },
  { id: "playlists", label: "歌单", icon: "playlist", group: "歌单", title: "歌单", description: "把音乐放在一起。" },
  { id: "downloads", label: "下载", icon: "download", group: "工具", title: "让音乐随时相伴", description: "下载管理将在下载功能接入后提供。" },
] as const satisfies readonly { id: string; label: string; icon: IconName; group: string; title: string; description: string }[];
export type PageKind = typeof destinations[number]["id"];
export type Route = { [K in Exclude<PageKind,"categories">]: { kind: K } }[Exclude<PageKind,"categories">]
  | { kind: "categories"; source?: string; category?: string }
  | { kind: "source"; source: string }
  | { kind: "playlist" | "album" | "artist" | "chart" | "program"; ref: EntityRef };
export function routeKey(route: Route): string {
  return "ref" in route ? `${route.kind}:${route.ref}` : route.kind === "source" ? `source:${route.source}` : route.kind === "categories" ? JSON.stringify([route.kind,route.source,route.category]) : route.kind;
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
