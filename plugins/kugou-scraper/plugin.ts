import { definePlugin, AuthRequired, NotFound, RateLimited, Upstream, ParseError } from 'haiku';
import type { Context } from 'haiku';
import { Buffer } from 'buffer';
import { inflateSync } from 'zlib';

type Song = {
  hash: string;
  songname?: string;
  songname_original?: string;
  singername?: string;
  album_name?: string;
  duration?: number;
  imgurl?: string;
  album_img?: string;
  trans_param?: { union_cover?: string };
};
const key = [64, 71, 97, 119, 94, 50, 116, 71, 81, 54, 49, 45, 206, 210, 110, 105];
async function request<T>(ctx: Context, url: string, query: Record<string, string | number>) {
  const response = await ctx.http.get(url, { query, throwOnError: false }).response();
  if (response.status === 401) throw new AuthRequired();
  if (response.status === 404) throw new NotFound();
  if (response.status === 429) throw new RateLimited({
    retryAfter: Number(response.headers.get('retry-after') || 0),
  });
  if (!response.ok) throw new Upstream({ status: response.status });
  try {
    return await response.json() as T;
  } catch {
    throw new ParseError('Invalid Kugou response');
  }
}
export default definePlugin({
  id: 'kugou.scraper',
  logo: 'logo.png',
  name: '酷狗刮削',
  version: '2.0.0',
  http: { headers: { 'User-Agent': 'Mozilla/5.0' } },
  scrape: {
    async search(query, ctx) {
      const result = await request<{ data?: { info?: Song[] } }>(
        ctx, 'https://mobileservice.kugou.com/api/v3/search/song', {
          version: 9108, plat: 0, showtype: 1, pagesize: 10, page: 1,
          keyword: [query.title, ...query.artists].join(' '),
        },
      );
      return (result.data?.info || []).filter(s => s.hash && (s.songname || s.songname_original))
        .map(s => ({
          id: s.hash.toLowerCase(),
          title: (s.songname || s.songname_original || '').trim(),
          artists: (s.singername || '').split(/[、\/,，&]+/).map(a => a.trim()).filter(Boolean),
          album: s.album_name?.trim() || undefined,
          cover: (s.trans_param?.union_cover || s.imgurl || s.album_img)
            ?.replace(/^http:\/\//, 'https://'),
          duration: s.duration && s.duration > 0 ? s.duration : undefined,
        }));
    },
    async lyrics(candidate, ctx) {
      const found = await request<{ candidates?: { id: string; accesskey: string }[] }>(
        ctx, 'https://lyrics.kugou.com/search', {
          ver: 1, man: 'yes', client: 'pc', hash: candidate.id,
        },
      );
      const first = found.candidates?.[0];
      if (!first) throw new NotFound('No lyrics candidate');
      const params = {
        charset: 'utf8', client: 'pc', ver: 1, id: first.id, accesskey: first.accesskey,
      };
      try {
        const krc = await request<{ content?: string; contenttype?: number }>(
          ctx, 'https://lyrics.kugou.com/download', { ...params, fmt: 'krc' },
        );
        if (krc.content && krc.contenttype === 0) {
          const bytes = Buffer.from(krc.content, 'base64').slice(4);
          for (let i = 0; i < bytes.length; i++) bytes[i] ^= key[i % key.length]!;
          const text = Buffer.from(inflateSync(bytes)).toString('utf-8');
          if (/\[\d+,\d+\]/.test(text)) return { format: 'krc', text };
        }
      } catch (error) {
        if (ctx.signal.aborted) throw error;
      }
      const lrc = await request<{ content?: string }>(
        ctx, 'https://lyrics.kugou.com/download', { ...params, fmt: 'lrc' },
      );
      const text = Buffer.from(lrc.content || '', 'base64').toString('utf-8');
      if (!text.trim()) throw new NotFound('Empty lyrics');
      return { format: 'lrc', text };
    },
  },
});
