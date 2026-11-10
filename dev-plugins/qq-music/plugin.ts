import { definePlugin, AuthRequired, ParseError } from 'haiku';
import type {
  Context,
  Track,
  Album,
  Artist,
  Playlist,
  Toplist,
  Page,
  PageArgs,
  Shelf,
  Lyrics,
} from 'haiku';
import { createProtocol, type Data } from './protocol.js';

const nativeId = (id: string) => String(id).replace(/^qq:[^:]+:/, '');
const pageNumber = (cursor?: string) => {
  const page = cursor === undefined ? 1 : Number(cursor);
  if (!Number.isSafeInteger(page) || page < 1) throw new ParseError('Invalid page cursor');
  return page;
};
const pageArgs = (ctx: PageArgs) => ({ page: pageNumber(ctx.cursor), pageSize: ctx.limit });
const track = (item: Data): Track => ({
  id: nativeId(item.trackId),
  title: item.title,
  artists: item.artists || [],
  cover: item.cover,
  duration: item.duration,
  album: item.album ? { title: item.album } : undefined,
});
const album = (item: Data): Album => ({
  id: nativeId(item.albumId),
  title: item.title,
  cover: item.cover,
  artists: item.artist ? [item.artist] : [],
  trackCount: item.trackCount,
});
const artist = (item: Data): Artist => ({
  id: nativeId(item.artistId),
  name: item.name,
  cover: item.cover,
});
const playlist = (item: Data): Playlist => ({
  id: nativeId(item.playlistId),
  title: item.title,
  cover: item.cover,
  trackCount: item.trackCount,
  creator: item.creator,
  description: item.description,
});
function page<T>(data: Data, key: string, map: (item: Data) => T, cursor?: string): Page<T> {
  return {
    items: (data[key] || []).map(map),
    total: data.total,
    next: data.hasMore ? String(pageNumber(cursor) + 1) : undefined,
  };
}
function lyrics(item: Data): Lyrics {
  return { format: item.format, text: item.text, translation: item.translation };
}
async function search<T>(
  kind: string,
  key: string,
  map: (item: Data) => T,
  args: PageArgs & { keyword: string },
  ctx: Context,
): Promise<Page<T>> {
  const result = await createProtocol(ctx).search({
    ...pageArgs(args),
    type: kind,
    keyword: args.keyword,
  });
  return page(result, key, map, args.cursor);
}
async function detail(id: string, kind: string, ctx: Context & PageArgs) {
  return createProtocol(ctx).getPlaylist({ ...pageArgs(ctx), playlistId: `qq:${kind}:${id}` });
}

export default definePlugin({
  id: 'qq.music',
  name: 'QQ 音乐',
  version: '2.0.0',
  author: 'Haiku',
  logo: 'logo.png',
  http: { cookies: true, headers: { Referer: 'https://y.qq.com/' } },
  auth: {
    qrcode: {
      variants: { qq: 'QQ', wx: '微信' },
      async start(variant, ctx) {
        return createProtocol(ctx).startQr(variant || 'qq');
      },
      async poll(state, ctx) {
        return createProtocol(ctx).pollQr(state);
      },
    },
    sms: {
      async send(phone, ctx) {
        return createProtocol(ctx).sendSms(phone);
      },
      async verify(state, code, ctx) {
        return createProtocol(ctx).verifySms(state, code);
      },
    },
    cookie: {
      async submit(value, ctx) {
        return createProtocol(ctx).cookie(value);
      },
    },
    async user(ctx) {
      return createProtocol(ctx).user();
    },
    async refresh(ctx) {
      return createProtocol(ctx).refresh();
    },
    async logout(ctx) {
      await createProtocol(ctx).logout();
    },
  },
  search: {
    track: (args, ctx) => search('track', 'tracks', track, args, ctx),
    album: (args, ctx) => search('album', 'albums', album, args, ctx),
    artist: (args, ctx) => search('artist', 'artists', artist, args, ctx),
    playlist: (args, ctx) => search('playlist', 'playlists', playlist, args, ctx),
  },
  track: {
    async get(id, ctx) {
      return track(await createProtocol(ctx).getTrack({ providerId: id }));
    },
    async streams(id, ctx) {
      const result = await createProtocol(ctx).getPlayback({
        providerId: id,
        quality: ctx.quality,
        preferFormat: ctx.format,
      });
      return result.urls.map((url: Data) => ({
        url: url.url,
        headers: url.headers,
        size: url.size,
        expiresAt: url.expiresAt,
        bitrate: url.bitrate,
        format:
          url.mimeType === 'audio/flac'
            ? 'flac'
            : url.mimeType === 'audio/ogg'
              ? 'ogg'
              : url.mimeType === 'audio/mp4'
                ? 'm4a'
                : 'mp3',
        quality:
          typeof url.quality === 'string'
            ? url.quality === 'lossless'
              ? 'lossless'
              : 'hires'
            : url.quality >= 320
              ? 'high'
              : 'standard',
      }));
    },
    async lyrics(id, ctx) {
      return lyrics(await createProtocol(ctx).getLyrics({ providerId: id }));
    },
  },
  album: {
    async get(id, ctx) {
      const data = await detail(id, 'album', ctx);
      return {
        info: {
          id,
          title: data.title,
          cover: data.cover,
          description: data.description,
          year: Number(data.year) || undefined,
        },
        tracks: page(data, 'tracks', track, ctx.cursor),
      };
    },
  },
  artist: {
    async get(id, ctx) {
      const data = await detail(id, 'singer', ctx);
      return {
        info: { id, name: data.title, cover: data.cover, description: data.description },
        tracks: page(data, 'tracks', track, ctx.cursor),
      };
    },
  },
  playlist: {
    async get(id, ctx) {
      const data = await detail(id, 'playlist', ctx);
      return { info: { ...playlist(data), id }, tracks: page(data, 'tracks', track, ctx.cursor) };
    },
  },
  toplist: {
    async list(ctx) {
      const data = await createProtocol(ctx).request({
        module: 'music.musicToplist.Toplist',
        method: 'GetAll',
        param: {},
      });
      return (data.group || []).flatMap((group: Data) =>
        (group.toplist || []).map(
          (item: Data): Toplist => ({
            id: String(item.topId),
            title: item.title,
            cover: item.frontPicUrl || item.headPicUrl,
          }),
        ),
      );
    },
    async get(id, ctx) {
      const data = await detail(id, 'top', ctx);
      return {
        info: { id, title: data.title, cover: data.cover },
        tracks: page(data, 'tracks', track, ctx.cursor),
      };
    },
  },
  async home(ctx) {
    const data = await createProtocol(ctx).getHomeFeed({
      cursor: ctx.cursor ? JSON.parse(ctx.cursor) : undefined,
      keepRadio: true,
    });
    const shelves: Shelf[] = [];
    for (const shelf of data.shelves) {
      const base = { id: shelf.id, title: shelf.title || '推荐' };
      if (shelf.type === 'song')
        shelves.push({ ...base, type: 'tracks', items: shelf.items.map(track) });
      if (shelf.type === 'playlist')
        shelves.push({ ...base, type: 'playlists', items: shelf.items.map(playlist) });
      if (shelf.type === 'album')
        shelves.push({ ...base, type: 'albums', items: shelf.items.map(album) });
      if (shelf.type === 'entry') {
        const items = shelf.items.filter((item: Data) => typeof item.target === 'string'
          && item.target.startsWith('qq:playlist:')).map((item: Data) => ({
          title: item.title, cover: item.cover, to: { playlist: nativeId(item.target) },
        }));
        if (items.length) shelves.push({ ...base, type: 'entries', items });
      }
      if (shelf.type === 'toplist')
        shelves.push({
          ...base,
          type: 'toplists',
          items: shelf.items.map((item: Data) => ({
            id: nativeId(item.toplistId),
            title: item.title,
            cover: item.cover,
          })),
        });
    }
    return { shelves, next: data.hasMore ? JSON.stringify(data.cursor) : undefined };
  },
  categories: {
    async list(ctx) {
      const data = await createProtocol(ctx).getDiscoverCategories();
      return {
        groups: data.groups.map((group: Data) => ({
          title: group.name,
          items: group.categories.map((item: Data) => ({ id: String(item.id), title: item.name })),
        })),
      };
    },
    async playlists(id, ctx) {
      const protocol = createProtocol(ctx);
      const categories = await protocol.getDiscoverCategories();
      const category = categories.groups
        .flatMap((group: Data) => group.categories)
        .find((item: Data) => String(item.id) === id);
      if (!category) throw new ParseError('Unknown QQ category');
      return page(
        await protocol.getRecommendedPlaylists({
          ...pageArgs(ctx),
          category: category.name,
        }),
        'playlists',
        playlist,
        ctx.cursor,
      );
    },
  },
  me: {
    async playlists(ctx) {
      return page(
        await createProtocol(ctx).getUserPlaylists(pageArgs(ctx)),
        'playlists',
        playlist,
        ctx.cursor,
      );
    },
    async likedTracks(ctx) {
      const protocol = createProtocol(ctx);
      if (!protocol.user()) throw new AuthRequired();
      const data = await protocol.request({
        module: 'music.musicasset.PlaylistBaseRead',
        method: 'GetPlaylistByUin',
        param: { uin: protocol.user()!.id },
      });
      const liked = data.v_playlist.find((item: Data) => item.dirId === 201 || item.dirid === 201);
      if (!liked) return { items: [] };
      return page(await detail(String(liked.tid), 'playlist', ctx), 'tracks', track, ctx.cursor);
    },
  },
  scrape: {
    async search(query, ctx) {
      const result = await search(
        'track',
        'tracks',
        track,
        {
          keyword: [query.title, ...query.artists].join(' '),
          limit: 10,
        },
        ctx,
      );
      return result.items.map((item) => ({
        id: item.id,
        title: item.title,
        artists: item.artists.map((a) => (typeof a === 'string' ? a : a.name)),
        album: item.album?.title,
        cover: item.cover,
        duration: item.duration,
      }));
    },
    async lyrics(candidate, ctx) {
      return lyrics(await createProtocol(ctx).getLyrics({ providerId: candidate.id }));
    },
  },
});
