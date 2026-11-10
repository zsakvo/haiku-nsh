import {
  definePlugin, field, AuthRequired, AuthExpired, NotFound, RateLimited, Upstream, Unsupported,
} from 'haiku';
import type { Context, Track, Album, Artist, Page, PageArgs, Stream } from 'haiku';
type Song = {
  id: string; title: string; artist?: string; artistId?: string; album?: string; albumId?: string;
  coverArt?: string; duration?: number; contentType?: string; suffix?: string; bitRate?: number;
};
type RemoteAlbum = {
  id: string; name: string; artist?: string; artistId?: string; coverArt?: string;
  year?: number; songCount?: number; song?: Song[];
};
type RemoteArtist = { id: string; name: string; coverArt?: string; album?: RemoteAlbum[] };
type Envelope = {
  status: string;
  error?: { code: number; message?: string };
  openSubsonic?: boolean;
  [key: string]: unknown;
};
const normalize = (raw: string) => {
  let value = raw.trim();
  if (!/^https?:\/\//i.test(value)) value = 'http://' + value;
  return new URL(value.replace(/\/+$/, '').replace(/\/(rest|app)$/i, ''))
    .toString().replace(/\/+$/, '');
};
async function credentials(ctx: Context, conn = ctx.connection) {
  if (!conn?.baseUrl || !conn.username || !conn.password) throw new AuthRequired();
  const cached = ctx.storage.get<{
    identity: string; salt: string; token: string;
  }>('subsonic.token');
  const identity = ctx.crypto.createHash('sha256').update(JSON.stringify(conn)).digest('hex');
  if (cached?.identity === identity) return {
    baseUrl: normalize(conn.baseUrl),
    username: conn.username,
    ...cached,
  };
  const salt = ctx.crypto.randomBytes(8).toString('hex');
  const token = ctx.crypto.createHash('md5').update(conn.password + salt).digest('hex');
  ctx.storage.set('subsonic.token', { identity, salt, token });
  return {
    baseUrl: normalize(conn.baseUrl),
    username: conn.username,
    identity, salt, token,
  };
}
async function apiUrl(
  ctx: Context, method: string, query: Record<string, string | number> = {}, conn = ctx.connection,
) {
  const c = await credentials(ctx, conn);
  const url = new URL(`${c.baseUrl}/rest/${method}`);
  for (const [key, value] of Object.entries({
    ...query, u: c.username, s: c.salt, t: c.token, v: '1.16.1', c: 'HaikuNSH', f: 'json',
  })) url.searchParams.set(key, String(value));
  return url.toString();
}
async function call(
  ctx: Context, method: string, query: Record<string, string | number> = {}, conn = ctx.connection,
): Promise<Envelope> {
  const response = await ctx.http
    .get(await apiUrl(ctx, method, query, conn), { throwOnError: false }).response();
  if (response.status === 401 || response.status === 403)
    throw method === 'ping' ? new AuthRequired() : new AuthExpired();
  if (response.status === 404) throw new NotFound();
  if (response.status === 429) throw new RateLimited({
    retryAfter: Number(response.headers.get('retry-after') || 0),
  });
  if (!response.ok) throw new Upstream({ status: response.status });
  const body = (await response.json()) as { 'subsonic-response'?: Envelope };
  const result = body['subsonic-response'];
  if (!result) throw new Upstream('Missing Subsonic envelope');
  if (result.status === 'failed') {
    const error = result.error;
    if ([40, 41, 50].includes(error?.code || 0))
      throw method === 'ping' ? new AuthRequired(error?.message) : new AuthExpired(error?.message);
    if (error?.code === 70) throw new NotFound(error.message);
    throw new Upstream(error?.message);
  }
  return result;
}
async function cover(ctx: Context, id?: string) {
  return id
    ? (await apiUrl(ctx, 'getCoverArt', { id, size: 'COVER_SIZE' })).replace('COVER_SIZE', '{size}')
    : undefined;
}
async function track(ctx: Context, song: Song): Promise<Track> {
  return {
    id: String(song.id),
    title: song.title,
    artists: song.artist ? [{ id: song.artistId, name: song.artist }] : [],
    album: song.album ? { id: song.albumId, title: song.album } : undefined,
    cover: await cover(ctx, song.coverArt),
    duration: song.duration,
    extra: { suffix: song.suffix || '', bitrate: song.bitRate || 0 },
  };
}
async function album(ctx: Context, value: RemoteAlbum): Promise<Album> {
  return {
    id: String(value.id),
    title: value.name,
    artists: value.artist ? [value.artist] : [],
    cover: await cover(ctx, value.coverArt),
    year: value.year,
    trackCount: value.songCount,
  };
}
async function artist(ctx: Context, value: RemoteArtist): Promise<Artist> {
  return {
    id: String(value.id),
    name: value.name,
    cover: await cover(ctx, value.coverArt),
  };
}
function page<T>(items: T[], args: PageArgs): Page<T> {
  const offset = Number(args.cursor || 0), end = offset + args.limit;
  return {
    items: items.slice(offset, end),
    next: end < items.length ? String(end) : undefined,
    total: items.length,
  };
}
function remotePage<T>(items: T[], args: PageArgs): Page<T> {
  return {
    items,
    next: items.length === args.limit ? String(Number(args.cursor || 0) + args.limit) : undefined,
  };
}
async function search(
  kind: 'song' | 'album' | 'artist', args: PageArgs & { keyword: string }, ctx: Context,
) {
  const result = await call(ctx, 'search3', {
    query: args.keyword,
    songCount: kind === 'song' ? args.limit : 0,
    albumCount: kind === 'album' ? args.limit : 0,
    artistCount: kind === 'artist' ? args.limit : 0,
    [`${kind}Offset`]: Number(args.cursor || 0),
  });
  const values = (result.searchResult3 || {}) as {
    song?: Song[]; album?: RemoteAlbum[]; artist?: RemoteArtist[];
  };
  if (kind === 'song')
    return remotePage(await Promise.all((values.song || []).map(v => track(ctx, v))), args);
  if (kind === 'album')
    return remotePage(await Promise.all((values.album || []).map(v => album(ctx, v))), args);
  return remotePage(await Promise.all((values.artist || []).map(v => artist(ctx, v))), args);
}
export default definePlugin({
  id: 'subsonic.music',
  name: 'Subsonic',
  version: '2.0.0',
  author: 'Haiku',
  description: 'Subsonic / OpenSubsonic music library',
  connection: {
    fields: {
      baseUrl: field.url('服务器地址'),
      username: field.text('用户名'),
      password: field.secret('密码'),
    },
    identity: ['baseUrl', 'username'],
    async verify(conn, ctx) {
      const result = await call(ctx, 'ping', {}, conn);
      ctx.storage.set('subsonic.open', !!result.openSubsonic);
      return {
        name: conn.username,
      };
    },
  },
  search: {
    track: (args, ctx) => search('song', args, ctx) as Promise<Page<Track>>,
    album: (args, ctx) => search('album', args, ctx) as Promise<Page<Album>>,
    artist: (args, ctx) => search('artist', args, ctx) as Promise<Page<Artist>>,
  },
  track: {
    async get(id, ctx) {
      const result = await call(ctx, 'getSong', { id });
      return track(ctx, result.song as Song);
    },
    async streams(id, ctx): Promise<Stream[]> {
      const result = await call(ctx, 'getSong', { id }), song = result.song as Song;
      const raw = !!ctx.storage.get('subsonic.open'), query: Record<string, string | number> = { id };
      if (raw) query.format = 'raw';
      if (ctx.format) query.format = ctx.format;
      if (ctx.quality === 'standard') query.maxBitRate = 128;
      else if (ctx.quality === 'high') query.maxBitRate = 320;
      const format = (ctx.format || song.suffix || 'mp3') as Stream['format'];
      return [{
        url: await apiUrl(ctx, 'stream', query),
        format,
        bitrate: song.bitRate ? song.bitRate * 1000 : undefined,
        quality: ctx.quality || 'standard',
      }];
    },
    async lyrics(id, ctx) {
      if (!ctx.storage.get('subsonic.open')) return null;
      try {
        const result = await call(ctx, 'getLyricsBySongId', { id });
        const lists = result.lyricsList as {
          structuredLyrics?: { synced?: boolean; line?: { start?: number; value: string }[] }[];
        } | undefined;
        const lyrics = lists?.structuredLyrics?.[0];
        if (!lyrics?.line?.length) return null;
        return {
          format: lyrics.synced ? 'lrc' : 'plain',
          text: lyrics.line.map(line => {
            if (!lyrics.synced) return line.value;
            const ms = line.start || 0;
            return `[${String(Math.floor(ms / 60000)).padStart(2, '0')}:${((ms % 60000) / 1000)
              .toFixed(2).padStart(5, '0')}]${line.value}`;
          }).join('\n'),
        };
      } catch (error) {
        if (error instanceof NotFound || error instanceof Unsupported) return null;
        throw error;
      }
    },
  },
  album: {
    async get(id, ctx) {
      const result = await call(ctx, 'getAlbum', { id }), value = result.album as RemoteAlbum;
      return {
        info: await album(ctx, value),
        tracks: page(await Promise.all((value.song || []).map(v => track(ctx, v))), ctx),
      };
    },
  },
  artist: {
    async get(id, ctx) {
      const result = await call(ctx, 'getArtist', { id });
      return {
        info: await artist(ctx, result.artist as RemoteArtist),
        tracks: { items: [] },
      };
    },
    async albums(id, ctx) {
      const result = await call(ctx, 'getArtist', { id });
      return page(await Promise.all(
        ((result.artist as RemoteArtist).album || []).map(v => album(ctx, v)),
      ), ctx);
    },
  },
  playlist: {
    async get(id, ctx) {
      const result = await call(ctx, 'getPlaylist', { id });
      const value = result.playlist as {
        id: string; name: string; owner?: string; songCount?: number; coverArt?: string; entry?: Song[];
      };
      return {
        info: {
          id: String(value.id),
          title: value.name,
          creator: value.owner,
          trackCount: value.songCount,
          cover: await cover(ctx, value.coverArt),
        },
        tracks: page(await Promise.all((value.entry || []).map(v => track(ctx, v))), ctx),
      };
    },
  },
  library: {
    cards: [
      { id: 'albums', label: '专辑', entity: 'album' },
      { id: 'artists', label: '艺术家', entity: 'artist' },
      { id: 'playlists', label: '歌单', entity: 'playlist' },
    ],
    async browse(card, ctx) {
      if (card === 'albums') {
        const result = await call(ctx, 'getAlbumList2', {
          type: 'alphabeticalByName', size: ctx.limit, offset: Number(ctx.cursor || 0),
        });
        return remotePage(await Promise.all(
          ((result.albumList2 as { album?: RemoteAlbum[] })?.album || []).map(v => album(ctx, v)),
        ), ctx);
      }
      if (card === 'artists') {
        const result = await call(ctx, 'getArtists');
        const values = ((result.artists as {
          index?: { artist?: RemoteArtist[] }[];
        })?.index || []).flatMap(v => v.artist || []);
        return page(await Promise.all(values.map(v => artist(ctx, v))), ctx);
      }
      if (card === 'playlists') {
        const result = await call(ctx, 'getPlaylists');
        const values = (result.playlists as {
          playlist?: { id: string; name: string; owner?: string; coverArt?: string }[];
        })?.playlist || [];
        const items = await Promise.all(values.map(async v => ({
          id: String(v.id),
          title: v.name,
          creator: v.owner,
          owned: v.owner === ctx.connection?.username,
          cover: await cover(ctx, v.coverArt),
        })));
        return page(items, ctx);
      }
      throw new Unsupported(card);
    },
  },
});
