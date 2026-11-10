import { definePlugin, AuthRequired, NotFound, RateLimited, Upstream, ParseError } from 'haiku';
import type { Context } from 'haiku';
import { Buffer } from 'buffer';

type Song = {
  mid: string;
  title?: string;
  name?: string;
  singer?: { name: string }[];
  album?: { mid?: string; name?: string; title?: string };
  interval?: number;
  time_public?: string;
  index_album?: number;
  index_cd?: number;
};
const base = 'https://c.y.qq.com';
async function request<T>(ctx: Context, path: string, query: Record<string, string | number>) {
  const response = await ctx.http.get(base + path, { query, throwOnError: false }).response();
  if (response.status === 401) throw new AuthRequired();
  if (response.status === 404) throw new NotFound();
  if (response.status === 429) throw new RateLimited({
    retryAfter: Number(response.headers.get('retry-after') || 0),
  });
  if (!response.ok) throw new Upstream({ status: response.status });
  try {
    return await response.json() as T;
  } catch {
    throw new ParseError('Invalid QQ response');
  }
}
export default definePlugin({
  id: 'qq.scraper',
  logo: 'logo.png',
  name: 'QQ 音乐刮削',
  version: '2.0.0',
  http: {
    headers: { Referer: 'https://y.qq.com/', 'User-Agent': 'Mozilla/5.0' },
  },
  scrape: {
    async search(query, ctx) {
      const result = await request<{ data?: { song?: { list?: Song[] } } }>(
        ctx, '/soso/fcgi-bin/client_search_cp', {
          format: 'json', new_json: 1, p: 1, n: 10,
          w: [query.title, ...query.artists].join(' '), t: 0, cr: 1,
        },
      );
      return (result.data?.song?.list || []).filter(s => s.mid && (s.title || s.name)).map(s => {
        const year = Number(s.time_public?.match(/^(\d{4})/)?.[1]);
        return {
          id: s.mid,
          title: (s.title || s.name || '').trim(),
          artists: (s.singer || []).map(a => a.name.trim()).filter(Boolean),
          album: s.album?.name || s.album?.title,
          cover: s.album?.mid
            ? `https://y.gtimg.cn/music/photo_new/T002R500x500M000${s.album.mid}.jpg`
            : undefined,
          duration: s.interval && s.interval > 0 ? s.interval : undefined,
          year: year >= 1900 && year <= 2200 ? year : undefined,
          track: s.index_album && s.index_album > 0 ? s.index_album : undefined,
          extra: { disc: s.index_cd && s.index_cd > 0 ? s.index_cd : null },
        };
      });
    },
    async lyrics(candidate, ctx) {
      const result = await request<{
        retcode?: number; code?: number; lyric?: string; trans?: string;
      }>(ctx, '/lyric/fcgi-bin/fcg_query_lyric_new.fcg', {
        format: 'json', songmid: candidate.id, g_tk: 5381, nobase64: 0,
      });
      if (result.retcode !== 0 && result.code !== 0) throw new NotFound();
      const text = Buffer.from(result.lyric || '', 'base64').toString('utf-8');
      if (!text.trim()) throw new NotFound('Empty lyrics');
      return {
        format: 'lrc',
        text,
        translation: result.trans
          ? Buffer.from(result.trans, 'base64').toString('utf-8') || undefined
          : undefined,
      };
    },
  },
});
