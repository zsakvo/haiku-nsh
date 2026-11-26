import type { PluginSnapshot } from "./plugins";
export const searchKinds = [
  {id:"track",label:"曲目"}, {id:"album",label:"专辑"}, {id:"artist",label:"歌手"},
  {id:"playlist",label:"歌单"}, {id:"program",label:"节目"},
] as const;
export type SearchKind = typeof searchKinds[number]["id"];
export interface SearchRequest { query: string; source?: string; kind?: SearchKind }
export interface SearchPageResult { items: any[]; next?: string | null; total?: number | null }
export interface SearchPreview { source: string; title: string; tracks: any[]; playlists: any[]; errors: string[] }
export function searchableSources(snapshot: PluginSnapshot) {
  return snapshot.sources.filter(s => s.enabled).flatMap(s => {
    const pkg = snapshot.packages.find(p => p.id===s.pluginId && p.enabled);
    const kinds = searchKinds.filter(k => pkg?.capabilities.includes(`search.${k.id}`));
    return pkg && kinds.length ? [{...s,title:s.displayName || pkg.name,kinds}] : [];
  });
}
/** Source highlight markup (`<em>…</em>`) split into plain and matched runs; other tags are dropped. */
export function highlightRuns(text: string): { text: string; match: boolean }[] {
  const runs: { text: string; match: boolean }[] = [];
  const pattern = /<em>([\s\S]*?)<\/em>/gi;
  let at = 0;
  const plain = (s: string) => s.replace(/<\/?[a-z][^>]*>/gi, "");
  for (let m = pattern.exec(text); m; m = pattern.exec(text)) {
    if (m.index > at) runs.push({ text: plain(text.slice(at, m.index)), match: false });
    runs.push({ text: plain(m[1]), match: true });
    at = m.index + m[0].length;
  }
  if (at < text.length) runs.push({ text: plain(text.slice(at)), match: false });
  return runs.filter((r) => r.text);
}
export const stripHighlight = (text: string) => highlightRuns(text).map((r) => r.text).join("");
