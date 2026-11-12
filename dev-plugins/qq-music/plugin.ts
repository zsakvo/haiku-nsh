// Ported from QQMusicApi 108617f (GPL-3.0).
// Maps QQ Music response fields directly to native-id Haiku entities and cursor pages.
// Authentication and transport live in auth.ts and api.ts.
import {
  definePlugin,
  ParseError,
  NotFound,
  VipRequired,
  Expired,
  Upstream,
  Unsupported,
} from 'haiku';
import type {
  Context,
  PageArgs,
  Page,
  Track,
  Album,
  Artist,
  Playlist,
  Toplist,
  Shelf,
  Stream,
  Quality,
  Format,
  Lyrics,
  Detail,
} from 'haiku';
import { api, batch, request, checkCode, credential, cookies } from './api.ts';
import { auth } from './auth.ts';
import { decryptQrc, randomHex } from './crypto.ts';
import type {
  Singer,
  RemoteAlbum,
  Song,
  RemotePlaylist,
  RemoteTop,
  SongPage,
  AlbumDetail,
  SingerDetail,
  Tags,
  PlaylistDetail,
  Card,
  Feed,
} from './types.ts';

interface FeedCursor {
  page: number;
  s_num: number;
  v_cache: string[];
}
interface Tier {
  prefix: string;
  format: Format;
  quality: Quality;
  size: (file: NonNullable<Song['file']>) => number | undefined;
  bitrate?: number;
}
const cover = (mid?: string, kind = 'T002') =>
  mid ? `https://y.gtimg.cn/music/photo_new/${kind}R{size}x{size}M000${mid}.jpg` : undefined;
const image = (url?: string) => url?.replace(/^http:/, 'https:');
const clean = (s = '') =>
  s.replace(
    /&(?:amp|lt|gt|quot|apos|#39);/g,
    (v) =>
      ({ '&amp;': '&', '&lt;': '<', '&gt;': '>', '&quot;': '"', '&apos;': "'", '&#39;': "'" })[v] ||
      v,
  );
function artist(s: Singer): Artist {
  const id = s.mid || s.singerMID;
  return {
    id,
    name: clean(s.name || s.singerName),
    cover: image(s.singerPic) || cover(id, 'T001'),
  };
}
function album(a: RemoteAlbum): Album {
  const id = a.mid || a.albumMid || a.albummid || String(a.id || a.albumID || '');
  return {
    id,
    title: clean(a.name || a.albumName),
    cover: image(a.pic) || cover(a.mid || a.albumMid || a.albummid),
    artists: (a.singer_list || a.singers || []).map(artist),
    year: Number(a.publishDate?.slice(0, 4)) || undefined,
    description: a.desc,
  };
}
function track(s: Song): Track {
  const a = s.album && album(s.album);
  return {
    id: s.mid || String(s.id),
    title: clean(s.title || s.name),
    artists: (s.singer || []).map(artist),
    album: a && { id: a.id, title: a.title },
    cover: a?.cover,
    duration: s.interval,
    extra: { songId: s.id },
    quality: [...new Set(TIERS.filter((t) => t.size(s.file || {})).map((t) => t.quality))],
  };
}
function playlist(p: RemotePlaylist): Playlist {
  return {
    id: String(p.id || p.tid || p.dissid || ''),
    title: clean(p.title || p.dirName || p.dissname || p.name),
    cover: image(p.picurl || p.picUrl || p.logo || p.imgurl || p.cover),
    trackCount: p.songnum ?? p.songNum,
    creator: clean(p.creator?.nick || p.nickname || p.nick),
    description: clean(p.desc || p.introduction),
  };
}
const toplist = (p: RemoteTop): Toplist => ({
  id: String(p.topId),
  title: p.title,
  cover: image(p.frontPicUrl || p.headPicUrl),
});
function offset(ctx: PageArgs): number {
  const n = Number(ctx.cursor || 0);
  if (!Number.isSafeInteger(n) || n < 0 || !Number.isSafeInteger(ctx.limit) || ctx.limit < 1)
    throw new ParseError('Invalid page cursor/limit');
  return n;
}
function page<T>(
  items: T[],
  ctx: PageArgs,
  total?: number,
  more = total === undefined ? items.length === ctx.limit : offset(ctx) + items.length < total,
): Page<T> {
  return {
    items,
    total,
    next: more && items.length ? String(offset(ctx) + items.length) : undefined,
  };
}
function detail<T>(
  info: T,
  songs: Song[],
  ctx: PageArgs,
  total: number,
  more?: boolean,
): Detail<T> {
  return { info, tracks: page(songs.map(track), ctx, total, more) };
}
async function search<R, T>(
  type: number,
  field: string,
  map: (value: R) => T,
  args: PageArgs & { keyword: string },
  ctx: Context,
): Promise<Page<T>> {
  let position = { page: 1, searchid: '' };
  if (args.cursor) {
    try {
      position = JSON.parse(args.cursor);
    } catch {
      throw new ParseError('Invalid search cursor');
    }
    if (
      !Number.isSafeInteger(position?.page) ||
      position.page < 1 ||
      typeof position.searchid !== 'string'
    )
      throw new ParseError('Invalid search cursor');
  }
  const value = await api<{
    meta: { sum: number; nextpage: number; searchid: string };
    body: { [key: string]: R[] };
  }>(ctx, 'music.search.SearchCgiService', 'DoSearchForQQMusicMobile', {
    // QQMusicApi SearchType: track=0, singer=1, album=2, playlist=3; selectors are optional mobile search filters.
    search_type: type,
    searchid: position.searchid || String(BigInt('0x' + randomHex(7))),
    query: args.keyword,
    num_per_page: args.limit,
    page_num: position.page,
    highlight: 0,
    grp: 1,
    selectors: {},
    vec_selectors: [],
  });
  const items = (value.body[field] || []).map(map);
  return {
    items,
    total: value.meta.sum,
    next:
      items.length && value.meta.nextpage > position.page
        ? JSON.stringify({ page: value.meta.nextpage, searchid: value.meta.searchid })
        : undefined,
  };
}
async function song(ctx: Context, id: string): Promise<Song> {
  const value = await api<{ track_info?: Song }>(
    ctx,
    'music.pf_song_detail_svr',
    'get_song_detail_yqq',
    /^\d+$/.test(id) ? { song_id: Number(id) } : { song_mid: id },
    { platform: 'web' },
  );
  if (!value.track_info) throw new NotFound();
  return value.track_info;
}
async function lyrics(id: string, ctx: Context): Promise<Lyrics | null> {
  const value = await api<{ lyric?: string; trans?: string }>(
    ctx,
    'music.musichallSong.PlayLyricInfo',
    'GetPlayLyricInfo',
    {
      ...(/^\d+$/.test(id) ? { songId: Number(id) } : { songMid: id }),
      crypt: 1,
      lrc_t: 0,
      qrc: 1,
      qrc_t: 0,
      roma: 0,
      roma_t: 0,
      trans: 1,
      trans_t: 0,
      needSingingAnnotations: false,
      type: 1,
    },
  );
  let text = decryptQrc(value.lyric || '');
  if (!text) return null;
  const content = text.match(/LyricContent="([\s\S]*?)"/)?.[1];
  if (content) text = clean(content.replace(/&#10;/g, '\n').replace(/&#13;/g, '\r'));
  // QRC word timestamps are absolute; Haiku's krc representation uses offsets from the line start.
  const lines = [...text.matchAll(/^\[(\d+),(\d+)\](.*)$/gm)]
    .map((m) => {
      const words = [...m[3].matchAll(/([^\n(]*?)\((\d+),(\d+)\)/g)];
      return words.length
        ? `[${m[1]},${m[2]}]` +
            words
              .map((w) => `<${Math.max(0, Number(w[2]) - Number(m[1]))},${w[3]},0>${w[1]}`)
              .join('')
        : '';
    })
    .filter(Boolean);
  return {
    format: lines.length ? 'krc' : 'lrc',
    text: lines.length ? lines.join('\n') : text,
    translation: decryptQrc(value.trans || '') || undefined,
  };
}
// QQMusicApi SongFileType prefixes; only containers playable by the host are requested.
const TIERS: Tier[] = [
  { prefix: 'AI00', format: 'flac', quality: 'hires', size: (f) => f.size_new?.[0] },
  { prefix: 'Q000', format: 'flac', quality: 'hires', size: (f) => f.size_new?.[1] },
  { prefix: 'F000', format: 'flac', quality: 'lossless', size: (f) => f.size_flac },
  { prefix: 'O801', format: 'ogg', quality: 'lossless', size: (f) => f.size_new?.[5] },
  { prefix: 'M800', format: 'mp3', quality: 'high', size: (f) => f.size_320mp3, bitrate: 320000 },
  { prefix: 'O800', format: 'ogg', quality: 'high', size: (f) => f.size_new?.[3], bitrate: 320000 },
  { prefix: 'O600', format: 'ogg', quality: 'high', size: (f) => f.size_192ogg, bitrate: 192000 },
  { prefix: 'C600', format: 'm4a', quality: 'high', size: (f) => f.size_192aac, bitrate: 192000 },
  {
    prefix: 'M500',
    format: 'mp3',
    quality: 'standard',
    size: (f) => f.size_128mp3,
    bitrate: 128000,
  },
  { prefix: 'O400', format: 'ogg', quality: 'standard', size: (f) => f.size_96ogg, bitrate: 96000 },
  { prefix: 'C400', format: 'm4a', quality: 'standard', size: (f) => f.size_96aac, bitrate: 96000 },
  { prefix: 'C200', format: 'm4a', quality: 'standard', size: (f) => f.size_48aac, bitrate: 48000 },
];
async function streams(
  id: string,
  ctx: Context & { quality: Quality; format?: Format },
): Promise<Stream[]> {
  const s = await song(ctx, id);
  const file = s.file || {};
  const qualities: Quality[] = ['hires', 'lossless', 'high', 'standard'];
  const tiers = TIERS.filter(
    (t) =>
      qualities.indexOf(t.quality) >= qualities.indexOf(ctx.quality || 'high') &&
      (t.size(file) || 0) > 0,
  );
  tiers.sort(
    (a, b) =>
      qualities.indexOf(a.quality) - qualities.indexOf(b.quality) ||
      Number(b.format === ctx.format) - Number(a.format === ctx.format),
  );
  const filename = (t: Tier) => `${t.prefix}${file.media_mid || s.mid + s.mid}.${t.format}`;
  if (!tiers.length) throw s.pay?.pay_play ? new VipRequired() : new Expired('QQ 未返回可播放文件');
  const c = credential(ctx);
  const value = await api<{
    expiration?: number;
    midurlinfo: { filename: string; result: number; purl: string }[];
  }>(ctx, 'music.vkey.GetVkey', 'UrlGetVkey', {
    uin: c.str_musicid || String(c.musicid),
    guid: randomHex(16),
    filename: tiers.map(filename),
    songmid: tiers.map(() => s.mid),
    songtype: tiers.map(() => s.type || 0),
    ctx: 0,
  });
  const candidates: Stream[] = [];
  for (const tier of tiers) {
    const url = value.midurlinfo.find((v) => v.filename === filename(tier));
    if (!url || url.result || !url.purl) continue;
    candidates.push({
      url: /^https?:\/\//.test(url.purl)
        ? url.purl
        : `https://isure.stream.qqmusic.qq.com/${url.purl.replace(/^\//, '')}`,
      format: tier.format,
      quality: tier.quality,
      bitrate: tier.bitrate,
      size: tier.size(file),
      expiresAt: value.expiration ? Math.floor(Date.now() / 1000) + value.expiration : undefined,
      headers: { Referer: 'https://y.qq.com/', Cookie: cookies(c) },
    });
  }
  if (!candidates.length) {
    const codes = value.midurlinfo.map((v) => v.result);
    for (const code of codes.filter((v) => [1000, 104400, 104401].includes(v))) checkCode(code);
    if (codes.includes(104003) || s.pay?.pay_play) throw new VipRequired();
    if (codes.includes(104013)) throw new Upstream('QQ 限制当前设备播放');
    throw new Expired('QQ 播放地址已失效');
  }
  return candidates;
}
async function playlistDetail(id: string, ctx: Context & PageArgs) {
  const value = await api<PlaylistDetail>(ctx, 'music.srfDissInfo.DissInfo', 'CgiGetDiss', {
    disstid: Number(id),
    dirid: 0,
    tag: 1,
    song_begin: offset(ctx),
    song_num: ctx.limit,
    userinfo: 1,
    orderlist: 1,
    onlysonglist: 0,
  });
  return detail(
    { ...playlist(value.dirinfo), id },
    value.songlist,
    ctx,
    value.total_song_num,
    !!value.hasmore,
  );
}
function feedPosition(cursor?: string): FeedCursor {
  if (!cursor) return { page: 1, s_num: 0, v_cache: [] };
  let value: FeedCursor;
  try {
    value = JSON.parse(cursor);
  } catch {
    throw new ParseError('Invalid home cursor');
  }
  if (
    !Number.isSafeInteger(value?.page) ||
    value.page < 1 ||
    !Number.isSafeInteger(value.s_num) ||
    value.s_num < 0 ||
    !Array.isArray(value.v_cache) ||
    value.v_cache.some((v) => typeof v !== 'string')
  )
    throw new ParseError('Invalid home cursor');
  return value;
}
function classify(card: Card) {
  if (!card.id || String(card.id) === '0') return;
  const type = Number(card.type);
  const subtype = Number(card.subtype);
  const jump = Number(card.jumptype);
  // RecommendFeed: 200/1700 songs, 500 playlists, 400 albums, 1000 charts; 510/513 are shortcuts.
  // Cards without a Haiku target are skipped: 900 mini-apps, 910/920 star/lyric cards,
  // 1100 live rooms and -1 web links.
  if ([200, 1700].includes(type) || [10046, 20007].includes(jump)) return 'track';
  if (type === 500 && [510, 513].includes(subtype)) return 'playlist-entry';
  if (type === 500 && jump === 10014) return 'playlist';
  // 10025 is a long-audio (audiobook) album; it opens through the regular album API.
  if (type === 400 || [10002, 10025].includes(jump)) return 'album';
  if (jump === 10005 || type === 1000) return 'toplist';
  if (jump === 20001) return 'radio-entry';
}
async function home(ctx: Context & { cursor?: string; refresh: boolean }) {
  const position = feedPosition(ctx.cursor);
  const feed = await api<Feed>(ctx, 'music.recommend.RecommendFeed', 'get_recommend_feed', {
    ...position,
    direction: ctx.cursor ? 1 : 0,
  });
  checkCode(feed.retcode);
  const shelves: Shelf[] = [];
  for (const shelf of feed.v_shelf) {
    const cards = shelf.v_niche
      .flatMap((n) => n.v_card)
      .flatMap((c) =>
        c.card_extra_info?.SongList?.length
          ? c.card_extra_info.SongList.map((s) => ({
              ...c,
              id: s.ID,
              title: s.Name,
              subtitle: s.SingerName,
              songMid: s.MID,
              cover: s.Cover,
            }))
          : [c],
      );
    const base = {
      id: String(shelf.id),
      // A bare "{String}" template with empty content would leave no title, and the host
      // drops untitled shelves.
      title:
        (shelf.title_template || shelf.title_content)
          .replace(/\{String\}/g, shelf.title_content)
          .trim() || '为你推荐',
    };
    const tracks: Track[] = [];
    const playlists: Playlist[] = [];
    const albums: Album[] = [];
    const tops: Toplist[] = [];
    const entries: Extract<Shelf, { type: 'entries' }>['items'] = [];
    for (const c of cards) {
      const kind = classify(c);
      if (!kind) continue;
      const id = String(c.id);
      const title = c.title || c.subtitle || '';
      const art = image(
        typeof c.cover === 'string' ? c.cover : c.cover?.default_url || c.cover?.medium_url,
      );
      if (kind === 'track') {
        const full = c.card_extra_info?.Tracks?.find(
          (s) => s.mid === c.songMid || String(s.id) === id,
        );
        tracks.push(
          full
            ? track(full)
            : { id: c.songMid || id, title, artists: c.subtitle ? [c.subtitle] : [], cover: art },
        );
      } else if (kind === 'playlist-entry')
        entries.push({ title, cover: art, to: { playlist: id } });
      else if (kind === 'playlist') playlists.push({ id, title, cover: art });
      else if (kind === 'album')
        albums.push({ id, title, cover: art, artists: c.subtitle ? [c.subtitle] : [] });
      else if (kind === 'toplist') tops.push({ id, title, cover: art });
      else if (kind === 'radio-entry') entries.push({ title, cover: art, to: { radio: '99' } });
    }
    if (tracks.length)
      shelves.push({ ...base, id: base.id + ':tracks', type: 'tracks', items: tracks });
    if (playlists.length)
      shelves.push({ ...base, id: base.id + ':playlists', type: 'playlists', items: playlists });
    if (albums.length)
      shelves.push({ ...base, id: base.id + ':albums', type: 'albums', items: albums });
    if (tops.length)
      shelves.push({ ...base, id: base.id + ':toplists', type: 'toplists', items: tops });
    if (entries.length)
      shelves.push({ ...base, id: base.id + ':entries', type: 'entries', items: entries });
  }
  return {
    shelves,
    next: feed.v_shelf.length
      ? JSON.stringify({
          page: position.page + 1,
          s_num: position.s_num + feed.v_shelf.length,
          v_cache: [...new Set([...position.v_cache, ...feed.v_shelf.map((s) => String(s.id))])],
        })
      : undefined,
  };
}
export default definePlugin({
  id: 'qq.music',
  name: 'QQ 音乐',
  version: '3.0.0',
  author: 'Haiku',
  logo: 'logo.png',
  http: { headers: { Referer: 'https://y.qq.com/' } },
  auth,
  search: {
    track: (args, ctx) => search(0, 'item_song', track, args, ctx),
    album: (args, ctx) => search(2, 'item_album', album, args, ctx),
    artist: (args, ctx) => search(1, 'singer', artist, args, ctx),
    playlist: (args, ctx) => search(3, 'item_songlist', playlist, args, ctx),
  },
  track: { get: async (id, ctx) => track(await song(ctx, id)), streams, lyrics },
  album: {
    async get(id, ctx) {
      const numeric = /^\d+$/.test(id);
      const key = numeric ? { albumId: Number(id) } : { albumMid: id };
      const [info, songs] = await batch<[AlbumDetail, SongPage]>(ctx, [
        {
          module: 'music.musichallAlbum.AlbumInfoServer',
          method: 'GetAlbumDetail',
          // AlbumDetail spells albumMId differently from AlbumSongList's albumMid.
          param: numeric ? { albumId: Number(id) } : { albumMId: id },
        },
        {
          module: 'music.musichallAlbum.AlbumSongList',
          method: 'GetAlbumSongList',
          param: { ...key, begin: offset(ctx), num: ctx.limit },
        },
      ]);
      const metadata = {
        ...album({ ...info.basicInfo, singers: info.singer?.singerList }),
        id,
        trackCount: songs.totalNum,
      };
      return detail(
        metadata,
        songs.songList.map((s) => s.songInfo),
        ctx,
        songs.totalNum,
      );
    },
  },
  artist: {
    async get(id, ctx) {
      const [info, songs] = await batch<[SingerDetail, SongPage]>(ctx, [
        {
          module: 'music.musichallSinger.SingerInfoInter',
          method: 'GetSingerDetail',
          param: {
            singer_mids: [id],
            group_singer: 1,
            wiki_singer: 1,
            ex_singer: 1,
            pic: 1,
            photos: 1,
          },
        },
        {
          module: 'musichall.song_list_server',
          method: 'GetSingerSongList',
          param: { singerMid: id, order: 1, number: ctx.limit, begin: offset(ctx) },
        },
      ]);
      const singer = info.singer_list[0];
      if (!singer) throw new NotFound();
      const metadata = {
        id,
        name: singer.basic_info.name,
        cover: image(singer.pic?.pic) || cover(id, 'T001'),
        description: singer.ex_info?.desc,
      };
      return detail(
        metadata,
        songs.songList.map((s) => s.songInfo),
        ctx,
        songs.totalNum,
      );
    },
  },
  playlist: { get: playlistDetail },
  toplist: {
    async list(ctx) {
      const value = await api<{ group: { toplist: RemoteTop[] }[] }>(
        ctx,
        'music.musicToplist.Toplist',
        'GetAll',
      );
      return value.group.flatMap((g) => g.toplist.map(toplist));
    },
    async get(id, ctx) {
      const value = await api<{ data: RemoteTop; songInfoList: Song[] }>(
        ctx,
        'music.musicToplist.Toplist',
        'GetDetail',
        { topId: Number(id), offset: offset(ctx), num: ctx.limit, withTags: true },
      );
      return detail(
        { ...toplist(value.data), id },
        value.songInfoList,
        ctx,
        value.data.totalNum || 0,
      );
    },
  },
  home,
  categories: {
    async list(ctx) {
      // QQMusicApi has no category API; preserve the public yqq tag CGI used by the existing capability.
      const response = await request(
        ctx,
        'https://c.y.qq.com/splcloud/fcgi-bin/fcg_get_diss_tag_conf.fcg',
        { query: { format: 'json', g_tk: 5381, inCharset: 'utf8', outCharset: 'utf-8' } },
      );
      const body = (await response.json()) as Tags;
      checkCode(body.code);
      return {
        sorts: [{ id: 'hot', title: '热门' }],
        groups: body.data.categories.map((g) => ({
          title: clean(g.categoryGroupName),
          items: g.items.map((c) => ({ id: String(c.categoryId), title: clean(c.categoryName) })),
        })),
      };
    },
    async playlists(id, ctx) {
      if (ctx.sort && ctx.sort !== 'hot') throw new Unsupported('Unknown playlist sort');
      const response = await request(
        ctx,
        'https://c.y.qq.com/splcloud/fcgi-bin/fcg_get_diss_by_tag.fcg',
        {
          query: {
            format: 'json',
            g_tk: 5381,
            inCharset: 'utf8',
            outCharset: 'utf-8',
            platform: 'yqq.json',
            needNewCode: 0,
            categoryId: id,
            sortId: 5,
            sin: offset(ctx),
            ein: offset(ctx) + ctx.limit - 1, // sortId=5 is the legacy yqq hot order.
          },
        },
      );
      const body = (await response.json()) as {
        code: number;
        data: { list: RemotePlaylist[]; sum: number };
      };
      checkCode(body.code);
      return page(body.data.list.map(playlist), ctx, body.data.sum);
    },
  },
  me: {
    async playlists(ctx) {
      const c = credential(ctx, true);
      const match = /^(owned|fav):(\d+)$/.exec(ctx.cursor || 'owned:0');
      if (!match) throw new ParseError('Invalid user playlist cursor');
      const args = { cursor: match[2], limit: ctx.limit };
      const start = offset(args);
      if (match[1] === 'owned') {
        const value = await api<{ v_playlist: RemotePlaylist[] }>(
          ctx,
          'music.musicasset.PlaylistBaseRead',
          'GetPlaylistByUin',
          { uin: c.str_musicid || String(c.musicid) },
        );
        const items = value.v_playlist
          .filter((p) => !p.invalid && (p.tid || p.id))
          .map((p) => ({ ...playlist(p), owned: true }));
        return {
          items: items.slice(start, start + ctx.limit),
          next: start + ctx.limit < items.length ? `owned:${start + ctx.limit}` : 'fav:0',
        };
      }
      const value = await api<{ v_list: RemotePlaylist[]; total: number; hasmore: number }>(
        ctx,
        'music.musicasset.PlaylistFavRead',
        'CgiGetPlaylistFavInfo',
        { uin: c.encrypt_uin, offset: start, size: ctx.limit },
      );
      const items = value.v_list
        .filter((p) => !p.invalid)
        .map((p) => ({ ...playlist(p), owned: false }));
      return {
        items,
        next:
          value.hasmore && value.v_list.length ? `fav:${start + value.v_list.length}` : undefined,
      };
    },
    async likedTracks(ctx) {
      const c = credential(ctx, true);
      // dirid 201 is QQMusicApi's special "我喜欢" playlist, addressed using enc_host_uin.
      const value = await api<PlaylistDetail>(ctx, 'music.srfDissInfo.DissInfo', 'CgiGetDiss', {
        disstid: 0,
        dirid: 201,
        tag: 1,
        song_begin: offset(ctx),
        song_num: ctx.limit,
        userinfo: 1,
        orderlist: 1,
        enc_host_uin: c.encrypt_uin,
      });
      return page(value.songlist.map(track), ctx, value.total_song_num, !!value.hasmore);
    },
  },
  radio: {
    async next(channelId, ctx) {
      // QQMusicApi's get_guess_recommend uses personal radio channel 99 and numeric song_ids as history.
      if (channelId !== undefined && channelId !== '99')
        throw new Unsupported('Unknown QQ radio channel');
      const songIds = ctx.history
        .map((t) => (t.extra as { songId?: number } | undefined)?.songId)
        .filter((id): id is number => !!id);
      const value = await api<{ tracks: Song[] }>(
        ctx,
        'music.radioProxy.MbTrackRadioSvr',
        'get_radio_track',
        { id: 99, num: 5, from: 0, scene: 0, song_ids: songIds },
      );
      const seen = new Set(ctx.history.map((t) => t.id));
      return value.tracks
        .map(track)
        .filter((t) => !seen.has(t.id))
        .slice(0, ctx.count);
    },
  },
  scrape: {
    async search(query, ctx) {
      const result = await search(
        0,
        'item_song',
        track,
        { keyword: [query.title, ...query.artists].join(' '), limit: 10 },
        ctx,
      );
      return result.items.map((t) => ({
        id: t.id,
        title: t.title,
        artists: t.artists.map((a) => (typeof a === 'string' ? a : a.name)),
        album: t.album?.title,
        cover: t.cover,
        duration: t.duration,
        extra: t.extra,
      }));
    },
    lyrics: (candidate, ctx) => lyrics(candidate.id, ctx),
  },
});
