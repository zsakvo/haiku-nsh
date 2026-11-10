import { definePlugin, field, AuthRequired, AuthExpired, NotFound, RateLimited, Upstream } from 'haiku';
import { Buffer } from 'buffer';
import type { Context, Track, Album, Artist, Playlist, Page, PageArgs, Stream } from 'haiku';

type Remote = {
  guid: string;
  name?: string;
  title?: string;
  coverId?: string;
  artists?: { guid?: string; name: string }[];
  album?: { guid?: string; name: string; coverId?: string };
  duration?: number;
  audioSpec?: { format?: string; bitrate?: number; size?: number };
  trackCount?: number;
  releaseDate?: string;
};
type Envelope<T> = { code: number; msg?: string; data: T };
type Session = {
  token: string;
  deviceId: string;
  baseUrl: string;
  relayMode: boolean;
  fnId?: string;
};
const prefix = '/music/api/v1';
const relayApi = 'https://5ddd.com/api/v1/fn/con';
function normalize(input: string) {
  let value = input.trim();
  if (!/^https?:\/\//i.test(value)) value = `http://${value}`;
  return value.replace(/\/+$/, '').replace(/\/music(?:\/api\/v1)?$/i, '');
}
async function resolve(ctx: Context, input: string) {
  const value = input.trim();
  // Match reference isFnId before URL normalization; identity still uses field.url.
  if (value.length < 6 || /^https?:\/\//i.test(value)) {
    return { baseUrl: normalize(value), relayMode: false };
  }
  const body = { fnId: value };
  const md5 = (text: string) => ctx.crypto.createHash('md5').update(text).digest('hex');
  const bytes = Array.from(ctx.crypto.randomBytes(4));
  const nonce = String(bytes.reduce((n, byte) => n * 256 + byte, 0) % 900000 + 100000);
  const timestamp = String(Date.now());
  const sign = md5([
    'NDzZTVxnRKP8Z0jXg1VAMonaG8akvh', '/api/v1/fn/con', nonce, timestamp,
    md5(JSON.stringify(body)), 'zIGtkc3dqZnJpd29qZXJqa2w7c',
  ].join('_'));
  const response = await ctx.http.post(relayApi, {
    json: body,
    headers: { authx: `nonce=${nonce}&timestamp=${timestamp}&sign=${sign}` },
    throwOnError: false,
  }).response();
  const result = await decode<{ fn?: string[] }>(response, true);
  const address = result.fn?.find(Boolean) || `${value}.5ddd.com`;
  const host = address.replace(/^https?:\/\//i, '').replace(/:\d+\/?$/, '')
    .replace(/\/+$/, '');
  if (!host) throw new Upstream('中继响应缺少地址');
  return { baseUrl: `https://${host}`, relayMode: true, fnId: value };
}
function base(ctx: Context) {
  if (!ctx.connection) throw new AuthRequired();
  const session = ctx.storage.get<Session>('feiniu.session');
  if (!session?.baseUrl) throw new AuthRequired();
  return session.baseUrl;
}
function endpoint(ctx: Context, path: string) {
  return base(ctx) + prefix + path;
}
function accessHeaders(ctx: Context): Record<string, string> {
  const code = ctx.connection?.accessCode;
  return code ? {
    'x-access-code': Buffer.from(code, 'utf8').toString('base64'),
    'x-access-source': 'app',
  } : {};
}
function headers(ctx: Context) {
  const session = ctx.storage.get<Session>('feiniu.session');
  if (!session?.token) throw new AuthRequired();
  return {
    ...accessHeaders(ctx),
    Cookie: `music-token=${session.token}${session.relayMode ? '; mode=relay' : ''}`,
  };
}
async function decode<T>(response: Response, login = false): Promise<T> {
  if (response.status === 401) throw login ? new AuthRequired() : new AuthExpired();
  if (response.status === 404) throw new NotFound();
  if (response.status === 429) throw new RateLimited({
    retryAfter: Number(response.headers.get('retry-after') || 0),
  });
  if (!response.ok) throw new Upstream({ status: response.status });
  const result = await response.json() as Envelope<T>;
  if (result.code === 401 || result.code === 120001)
    throw login ? new AuthRequired(result.msg) : new AuthExpired(result.msg);
  if (result.code === 404) throw new NotFound(result.msg);
  if (result.code !== 0) throw new Upstream(result.msg);
  return result.data;
}
async function login(ctx: Context, conn = ctx.connection) {
  if (!conn?.username || !conn.password) throw new AuthRequired();
  const deviceId = ctx.storage.get<Session>('feiniu.session')?.deviceId
    || ctx.crypto.randomBytes(16).toString('hex');
  const connection = await resolve(ctx, conn.baseUrl);
  const response = await ctx.http.post(connection.baseUrl + prefix + '/user/password-login', {
    headers: {
      ...accessHeaders({ ...ctx, connection: conn }),
      ...(connection.relayMode ? { Cookie: 'mode=relay' } : {}),
    },
    json: {
      username: conn.username.trim(),
      password: ctx.crypto.createHash('sha256').update(conn.password).digest('hex'),
      deviceId,
    },
    throwOnError: false,
  }).response();
  const result = await decode<{ userToken: string }>(response, true);
  if (!result.userToken) throw new AuthRequired('Missing user token');
  ctx.storage.set('feiniu.session', { ...connection, token: result.userToken, deviceId });
  return { token: result.userToken };
}
async function request<T>(ctx: Context, path: string, query: Record<string, string | number> = {}) {
  const response = await ctx.http.get(endpoint(ctx, path), {
    query, headers: headers(ctx), throwOnError: false,
  }).response();
  return decode<T>(response);
}
function cover(ctx: Context, id?: string) {
  return id ? `${endpoint(ctx, '/static/cover')}?coverId=${encodeURIComponent(id)}&size={size}`
    : undefined;
}
function track(ctx: Context, value: Remote): Track {
  return {
    id: value.guid,
    title: value.title || value.name || '',
    artists: (value.artists || []).map(a => ({ id: a.guid, name: a.name })),
    album: value.album ? { id: value.album.guid, title: value.album.name } : undefined,
    cover: cover(ctx, value.coverId || value.album?.coverId),
    duration: value.duration,
  };
}
function album(ctx: Context, value: Remote): Album {
  return {
    id: value.guid,
    title: value.name || '',
    artists: (value.artists || []).map(a => ({ id: a.guid, name: a.name })),
    cover: cover(ctx, value.coverId),
    year: value.releaseDate ? Number(value.releaseDate.slice(0, 4)) : undefined,
    trackCount: value.trackCount,
  };
}
function artist(ctx: Context, value: Remote): Artist {
  return {
    id: value.guid,
    name: value.name || '',
    cover: cover(ctx, value.coverId),
  };
}
function playlist(ctx: Context, value: Remote): Playlist {
  return {
    id: value.guid,
    title: value.name || '',
    cover: cover(ctx, value.coverId),
    trackCount: value.trackCount,
    owned: true,
  };
}
async function list<T>(
  ctx: Context & PageArgs, path: string, map: (value: Remote) => T,
  query: Record<string, string | number> = {},
): Promise<Page<T>> {
  const page = Number(ctx.cursor || 1);
  const data = await request<{ list?: Remote[]; total?: number }>(ctx, path, {
    ...query, page, size: ctx.limit,
  });
  const items = (data.list || []).map(map);
  return {
    items,
    total: data.total,
    next: page * ctx.limit < (data.total ?? items.length) ? String(page + 1) : undefined,
  };
}
async function detail(ctx: Context & PageArgs, kind: 'album' | 'artist' | 'playlist', id: string) {
  const meta = await request<{ list?: Remote[] }>(ctx, `/${kind}/list`, { page: 1, size: -1 });
  const value = meta.list?.find(v => v.guid === id);
  if (!value) throw new NotFound();
  const info = kind === 'album' ? album(ctx, value)
    : kind === 'artist' ? artist(ctx, value) : playlist(ctx, value);
  const tracks = await list(ctx, `/track/${kind}-detail/list`, v => track(ctx, v), {
    [`${kind}GUID`]: id,
  });
  return { info, tracks };
}
export default definePlugin({
  id: 'feiniu.music',
  logo: 'logo.png',
  name: '飞牛音乐',
  version: '2.0.0',
  connection: {
    fields: {
      baseUrl: field.url('服务器地址或 FNID'),
      username: field.text('用户名'),
      password: field.secret('密码'),
      accessCode: field.secret('安全码（可选）', { required: false }),
    },
    identity: ['baseUrl', 'username'],
    async verify(conn, ctx) {
      await login(ctx, conn);
      return { name: conn.username };
    },
  },
  auth: {
    async user() {
      return null;
    },
    refresh: ctx => login(ctx),
  },
  search: {
    track: (args, ctx) => list({ ...ctx, ...args }, '/search/track', v => track(ctx, v), {
      q: args.keyword,
    }),
    album: (args, ctx) => list({ ...ctx, ...args }, '/search/album', v => album(ctx, v), {
      q: args.keyword,
    }),
    artist: (args, ctx) => list({ ...ctx, ...args }, '/search/artist', v => artist(ctx, v), {
      q: args.keyword,
    }),
  },
  track: {
    async get(id, ctx) {
      const data = await request<{ track?: Remote } & Remote>(ctx, '/track/metadata', { guid: id });
      return track(ctx, data.track || data);
    },
    async streams(id, ctx): Promise<Stream[]> {
      const data = await request<{ track?: Remote; audioSpec?: Remote['audioSpec'] }>(
        ctx, '/track/metadata', { guid: id },
      );
      const spec = data.audioSpec || data.track?.audioSpec;
      let url = `${endpoint(ctx, '/track/stream')}?guid=${encodeURIComponent(id)}`;
      let format = ctx.format || spec?.format || 'mp3';
      if (ctx.format || ['ape', 'dsf', 'dff', 'wma'].includes(format)) {
        format = ctx.format || 'flac';
        const response = await ctx.http.post(endpoint(ctx, '/track/transcode'), {
          headers: headers(ctx), throwOnError: false,
          json: { guid: id, output: { codec: format, channel: 2 } },
        }).response();
        const result = await decode<{ url: string }>(response);
        url = new URL(result.url, base(ctx)).toString();
      }
      return [{
        url,
        headers: headers(ctx),
        format: format as Stream['format'],
        quality: ctx.quality,
        bitrate: spec?.bitrate,
        size: spec?.size,
      }];
    },
    async lyrics(id, ctx) {
      const result = await request<{
        preferred?: string; list?: { guid: string; content?: string; isLRC?: boolean }[];
      }>(ctx, '/lyric/list', { trackGUID: id });
      const lyric = result.list?.find(v => v.guid === result.preferred) || result.list?.[0];
      return lyric?.content ? {
        format: lyric.isLRC === false ? 'plain' : 'lrc', text: lyric.content,
      } : null;
    },
  },
  album: {
    get: (id, ctx) => detail(ctx, 'album', id) as Promise<{
      info: Album; tracks: Page<Track>;
    }>,
  },
  artist: {
    get: (id, ctx) => detail(ctx, 'artist', id) as Promise<{
      info: Artist; tracks: Page<Track>;
    }>,
  },
  playlist: {
    get: (id, ctx) => detail(ctx, 'playlist', id) as Promise<{
      info: Playlist; tracks: Page<Track>;
    }>,
  },
  library: {
    cards: [
      { id: 'track', label: '歌曲', entity: 'track' },
      { id: 'album', label: '专辑', entity: 'album' },
      { id: 'artist', label: '艺术家', entity: 'artist' },
      { id: 'playlist', label: '歌单', entity: 'playlist' },
    ],
    async browse(card, ctx) {
      if (card === 'track') return list(ctx, '/track/list', v => track(ctx, v));
      if (card === 'album') return list(ctx, '/album/list', v => album(ctx, v));
      if (card === 'artist') return list(ctx, '/artist/list', v => artist(ctx, v));
      if (card === 'playlist') return list(ctx, '/playlist/list', v => playlist(ctx, v));
      throw new NotFound('Unknown library card');
    },
  },
});
