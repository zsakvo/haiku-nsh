// Built against MoeKoeMusic and MakcRe/KuGouMusicApi 5d6c9f7 (--platform=lite).
// Maps Kugou Lite gateway responses directly to Haiku v2 native-id entities and cursor pages.
import {
  definePlugin,
  AuthExpired,
  Expired,
  GeoRestricted,
  NotFound,
  ParseError,
  Unsupported,
  VipRequired,
} from 'haiku';
import type {
  Album,
  Artist,
  Context,
  Detail,
  Format,
  Lyrics,
  Page,
  PageArgs,
  Playlist,
  Quality,
  Shelf,
  Stream,
  Toplist,
  Track,
} from 'haiku';
import { Buffer } from 'buffer';
import { credential, ensureDevice, kugou } from './api.ts';
import { auth } from './auth.ts';
import { APPID, CLIENTVER, decodeKrc, signParamsKey } from './crypto.ts';
import type {
  AlbumSong,
  ArtistSong,
  Envelope,
  HomeDiscoverItem,
  LyricCandidate,
  PlaylistTagGroup,
  PlaylistTrack,
  PrivilegeItem,
  RankItem,
  RankSong,
  RecommendSong,
  RemoteSinger,
  SearchAlbum,
  SearchPlaylist,
  SearchSong,
  SongUrlResponse,
  TopIpItem,
  YouthCardData,
  YouthChannel,
} from './types.ts';

const QUALITIES: Quality[] = ['hires', 'lossless', 'high', 'standard'];
const UPSTREAM_QUALITIES = ['128', '320', 'flac', 'high', 'viper_atmos', 'viper_tape', 'viper_clear', 'super'];
const QUALITY_TO_UPSTREAM: Record<Quality, string> = {
  hires: 'high',
  lossless: 'flac',
  high: '320',
  standard: '128',
};
const UPSTREAM_TO_QUALITY: Record<string, Quality> = {
  high: 'hires',
  flac: 'lossless',
  '320': 'high',
  '128': 'standard',
};

// Youth (概念版) recommendation cards & EchoMusic radio channels.
const YOUTH_RADIO_CHANNELS = [
  { id: 'fm', title: '红心 Radio', subtitle: '根据你的听歌口味推荐' },
  { id: 'daily', title: '每日推荐', subtitle: '每天为你量身定制' },
  { id: 'youth:3005', title: '潮流尝鲜', subtitle: '概念版潮流新声' },
  { id: 'youth:3101', title: '概念er新推', subtitle: '概念er 精选推荐' },
  { id: 'youth:3001', title: '私人专属好歌', subtitle: '私人专属好歌推荐' },
  { id: 'youth:3004', title: '小众宝藏佳作', subtitle: '小众宝藏佳作发现' },
  { id: 'youth:3102', title: '同好风格推荐', subtitle: '喜欢同风格的TA也喜欢' },
  { id: 'youth:3006', title: 'VIP 专属推荐', subtitle: 'VIP 专属音乐推荐' },
];
const LEGACY_CARD_IDS = ['1', '2', '3', '4', '6'];

const clean = (value = ''): string =>
  value
    .replace(/<\/?em>/gi, '')
    .replace(
      /&(?:amp|lt|gt|quot|apos|#39);/g,
      (m) =>
        ({ '&amp;': '&', '&lt;': '<', '&gt;': '>', '&quot;': '"', '&apos;': "'", '&#39;': "'" })[
          m
        ] || m,
    )
    .trim();

function cover(url?: string): string | undefined {
  if (!url) return undefined;
  return url
    .replace(/^http:/, 'https:')
    .replace(/_\{\s*size\s*\}x\{\s*size\s*\}\.jpg$/i, '')
    .replace(/\/(100|120|150|240|400|480)\//, '/{size}/');
}

function durationSec(raw?: number): number | undefined {
  if (!raw || !Number.isFinite(raw) || raw <= 0) return undefined;
  // Kugou endpoints return seconds on search/privilege and milliseconds on album/artist/playlist/rank.
  return raw > 36000 ? Math.round(raw / 1000) : Math.round(raw);
}

function parsePage(ctx: PageArgs): number {
  const page = Number(ctx.cursor || 1);
  if (!Number.isSafeInteger(page) || page < 1 || !Number.isSafeInteger(ctx.limit) || ctx.limit < 1) {
    throw new ParseError('Invalid page cursor/limit');
  }
  return page;
}

function paged<T>(items: T[], page: number, limit: number, total?: number): Page<T> {
  const hasMore =
    items.length > 0 &&
    (total !== undefined ? page * limit < total : items.length >= limit);
  return {
    items,
    total,
    next: hasMore ? String(page + 1) : undefined,
  };
}

function splitArtistNames(raw?: string): string[] {
  if (!raw) return [];
  return clean(raw)
    .split(/[、/,&]|\s+-\s+/)
    .map((s) => s.trim())
    .filter(Boolean);
}

function mapArtists(list?: RemoteSinger[], fallback?: string): Artist[] | string[] {
  const mapped = (list || [])
    .map((s) => ({
      id: s.author_id || s.AuthorId || s.id ? String(s.author_id || s.AuthorId || s.id) : undefined,
      name: clean(s.author_name || s.AuthorName || s.name),
      cover: cover(s.sizable_avatar || s.Avatar || s.avatar),
    }))
    .filter((a) => a.name);
  return mapped.length ? mapped : splitArtistNames(fallback);
}

function splitTitleAuthor(fullName = ''): { title: string; author: string } {
  const text = clean(fullName);
  const idx = text.indexOf(' - ');
  if (idx > 0) return { author: text.slice(0, idx).trim(), title: text.slice(idx + 3).trim() };
  return { title: text, author: '' };
}

function qualitiesFromHashes(flags: {
  high?: string;
  flac?: string;
  q320?: string;
  q128?: string;
}): Quality[] {
  const out: Quality[] = [];
  if (flags.high) out.push('hires');
  if (flags.flac) out.push('lossless');
  if (flags.q320) out.push('high');
  if (flags.q128 || !out.length) out.push('standard');
  return out;
}

function mapSearchTrack(s: SearchSong): Track | null {
  const hash = (s.FileHash || s.HQFileHash || s.SQFileHash || '').toLowerCase();
  if (!hash) return null;
  const fallbackTitle = splitTitleAuthor(s.FileName).title;
  const albumId = s.AlbumID ? String(s.AlbumID) : undefined;
  return {
    id: hash,
    title: clean(s.OriSongName || s.SongName) || fallbackTitle,
    artists: mapArtists(s.Singers, s.SingerName),
    album: s.AlbumName ? { id: albumId && albumId !== '0' ? albumId : undefined, title: clean(s.AlbumName) } : undefined,
    cover: cover(s.Image),
    duration: durationSec(s.Duration),
    quality: qualitiesFromHashes({
      high: s.ResFileHash,
      flac: s.SQFileHash,
      q320: s.HQFileHash,
      q128: s.FileHash,
    }),
    extra: {
      albumId: albumId || '',
      albumAudioId: s.MixSongID ? String(s.MixSongID) : '',
    },
  };
}

function mapAlbum(a: SearchAlbum): Album | null {
  const id = String(a.albumid ?? a.album_id ?? '');
  if (!id || id === '0') return null;
  const date = a.publish_date || a.publish_time || a.publishtime || '';
  const year = Number(date.slice(0, 4)) || undefined;
  return {
    id,
    title: clean(a.album_name || a.albumname),
    artists: mapArtists(a.authors || a.singers, a.author_name || a.singername),
    cover: cover(a.sizable_cover || a.imgurl || a.img),
    year,
    trackCount: a.song_count ?? a.songcount,
    description: clean(a.intro) || undefined,
  };
}

function mapArtist(a: RemoteSinger): Artist | null {
  const id = String(a.author_id ?? a.AuthorId ?? a.id ?? '');
  const name = clean(a.author_name || a.AuthorName || a.name);
  if (!name) return null;
  return {
    id: id && id !== '0' ? id : undefined,
    name,
    cover: cover(a.sizable_avatar || a.Avatar || a.avatar),
    description: clean(a.intro) || undefined,
  };
}

function mapPlaylist(p: SearchPlaylist, owned?: boolean): Playlist | null {
  const id = String(p.global_collection_id || p.gid || p.list_create_gid || p.specialid || p.listid || '');
  if (!id || id === '0') return null;
  return {
    id,
    title: clean(p.specialname || p.name),
    cover: cover(p.flexible_cover || p.pic || p.img),
    creator: clean(p.list_create_username || p.nickname) || undefined,
    trackCount: p.song_count ?? p.count,
    description: clean(p.intro) || undefined,
    ...(owned !== undefined ? { owned } : {}),
  };
}

function mapPrivilegeTrack(item: PrivilegeItem): Track | null {
  const hash = (item.hash || '').toLowerCase();
  if (!hash) return null;
  const parsed = splitTitleAuthor(item.name);
  const albumId = item.album_id ? String(item.album_id) : undefined;
  const goods = item.relate_goods || [];
  const available = [
    ...new Set(
      goods
        .map((g) => (g.quality ? UPSTREAM_TO_QUALITY[g.quality] : undefined))
        .filter((q): q is Quality => !!q),
    ),
  ];
  return {
    id: hash,
    title: clean(item.songname) || parsed.title || clean(item.name),
    artists: splitArtistNames(item.singername || parsed.author),
    album: item.albumname
      ? { id: albumId && albumId !== '0' ? albumId : undefined, title: clean(item.albumname) }
      : undefined,
    cover: cover(item.trans_param?.union_cover || item.info?.image),
    duration: durationSec(item.info?.duration),
    quality: available.length ? available : ['standard'],
    extra: {
      albumId: albumId || '',
      albumAudioId: item.album_audio_id ? String(item.album_audio_id) : '',
    },
  };
}

function mapAlbumSong(s: AlbumSong): Track | null {
  const hash = (s.audio_info?.hash || s.audio_info?.hash_128 || '').toLowerCase();
  if (!hash) return null;
  const albumId = s.album_info?.album_id ? String(s.album_info.album_id) : undefined;
  return {
    id: hash,
    title: clean(s.base?.audio_name),
    artists: mapArtists(s.authors, s.base?.author_name),
    album: s.album_info?.album_name
      ? { id: albumId && albumId !== '0' ? albumId : undefined, title: clean(s.album_info.album_name) }
      : undefined,
    cover: cover(s.trans_param?.union_cover),
    duration: durationSec(s.audio_info?.duration),
    quality: qualitiesFromHashes({
      high: s.audio_info?.hash_high,
      flac: s.audio_info?.hash_flac,
      q320: s.audio_info?.hash_320,
      q128: s.audio_info?.hash,
    }),
    extra: {
      albumId: albumId || '',
      albumAudioId: s.base?.album_audio_id ? String(s.base.album_audio_id) : '',
    },
  };
}

function mapArtistSong(s: ArtistSong): Track | null {
  const hash = (s.hash || '').toLowerCase();
  if (!hash) return null;
  const albumId = s.album_id ? String(s.album_id) : undefined;
  return {
    id: hash,
    title: clean(s.audio_name),
    artists: mapArtists(s.authors, s.author_name),
    album: s.album_name
      ? { id: albumId && albumId !== '0' ? albumId : undefined, title: clean(s.album_name) }
      : undefined,
    cover: cover(s.trans_param?.union_cover),
    duration: durationSec(s.timelength),
    quality: qualitiesFromHashes({
      high: s.hash_high,
      flac: s.hash_flac,
      q320: s.hash_320,
      q128: s.hash,
    }),
    extra: {
      albumId: albumId || '',
      albumAudioId: s.album_audio_id ? String(s.album_audio_id) : '',
    },
  };
}

function mapPlaylistTrack(s: PlaylistTrack): Track | null {
  const hash = (s.hash || '').toLowerCase();
  if (!hash) return null;
  const parsed = splitTitleAuthor(s.name);
  const albumId = s.albuminfo?.id ? String(s.albuminfo.id) : undefined;
  const available = [
    ...new Set(
      (s.relate_goods || [])
        .map((g) => (g.quality ? UPSTREAM_TO_QUALITY[g.quality] : undefined))
        .filter((q): q is Quality => !!q),
    ),
  ];
  return {
    id: hash,
    title: parsed.title || clean(s.name),
    artists: mapArtists(s.singerinfo, parsed.author),
    album: s.albuminfo?.name
      ? { id: albumId && albumId !== '0' ? albumId : undefined, title: clean(s.albuminfo.name) }
      : undefined,
    cover: cover(s.cover),
    duration: durationSec(s.timelen),
    quality: available.length ? available : undefined,
    extra: {
      albumId: albumId || '',
      albumAudioId: s.mixsongid ? String(s.mixsongid) : '',
    },
  };
}

function mapRankSong(s: RankSong): Track | null {
  const hash = (s.deprecated?.hash || '').toLowerCase();
  if (!hash) return null;
  const albumId = s.album_id ? String(s.album_id) : undefined;
  return {
    id: hash,
    title: clean(s.songname),
    artists: mapArtists(s.authors, s.author_name),
    album: s.album_name
      ? { id: albumId && albumId !== '0' ? albumId : undefined, title: clean(s.album_name) }
      : undefined,
    cover: cover(s.trans_param?.union_cover),
    duration: durationSec(s.deprecated?.duration),
    quality: qualitiesFromHashes({
      high: s.deprecated?.hash_high,
      flac: s.deprecated?.hash_flac,
      q320: s.deprecated?.hash_320,
      q128: s.deprecated?.hash,
    }),
    extra: {
      albumId: albumId || '',
      albumAudioId: s.mixsongid ? String(s.mixsongid) : '',
    },
  };
}

function mapRecommendSong(s: RecommendSong): Track | null {
  const ai = s.audio_info;
  const hash = (ai?.hash || ai?.hash_128 || s.hash || s.hash_128 || s.hash_320 || '').toLowerCase();
  if (!hash) return null;
  const parsed = splitTitleAuthor(s.filename);
  const rawAlbumId = s.album_info?.album_id ?? s.album_id ?? s.ori_album_id;
  const albumId = rawAlbumId ? String(rawAlbumId) : undefined;
  const albumName = clean(s.album_info?.album_name || s.album_name);
  const rawDuration = Number(
    s.time_length ??
      s.timelength ??
      ai?.timelength ??
      ai?.timelength_128 ??
      s.timelength_128 ??
      s.duration ??
      ai?.duration ??
      0,
  );
  const rawCover =
    s.album_info?.sizable_cover ||
    s.album_info?.cover ||
    ai?.trans_param?.union_cover ||
    s.sizable_cover ||
    s.album_sizable_cover ||
    s.album_cover ||
    s.trans_param?.union_cover ||
    s.cover ||
    (s.pic && !s.pic.includes(',') ? s.pic : undefined);
  return {
    id: hash,
    title: clean(s.ori_audio_name || s.songname || s.audio_name || s.name) || parsed.title,
    artists: mapArtists(s.authors || s.singerinfo, s.author_name || s.singername || parsed.author),
    album: albumName
      ? { id: albumId && albumId !== '0' ? albumId : undefined, title: albumName }
      : undefined,
    cover: cover(rawCover),
    duration: durationSec(rawDuration),
    quality: qualitiesFromHashes({
      high: ai?.hash_high || s.hash_high,
      flac: ai?.hash_flac || s.hash_flac,
      q320: ai?.hash_320 || s.hash_320,
      q128: hash,
    }),
    extra: {
      albumId: albumId || '',
      albumAudioId: s.album_audio_id || s.mixsongid ? String(s.album_audio_id || s.mixsongid) : '',
    },
  };
}

function mapYouthChannel(ch: YouthChannel): Playlist | null {
  const gid = String(ch.global_collection_id || '');
  if (!gid || gid === '0') return null;
  return {
    id: `ch:${gid}`,
    title: clean(ch.name || ch.channel_name || ch.title),
    cover: cover(ch.sizable_cover || ch.cover || ch.pic || ch.img),
    creator: clean(ch.nickname || ch.create_user_name) || undefined,
    trackCount: ch.song_count ?? ch.count,
    description: clean(ch.intro || ch.desc) || undefined,
  };
}

function mapTopIpPlaylist(item: TopIpItem): Playlist | null {
  if (item.type !== 1) return null;
  const gid = String(
    item.extra?.global_collection_id || item.extra?.global_special_id || item.extra?.specialid || '',
  );
  if (!gid || gid === '0') return null;
  return {
    id: gid,
    title: clean(item.title),
    cover: cover(item.image_url),
    description: clean(item.sub_title) || undefined,
  };
}

function mapToplist(r: RankItem): Toplist | null {
  const id = String(r.rankid ?? '');
  if (!id || id === '0') return null;
  const subtitle = (r.intro || '')
    .split('\n')
    .map((line) => line.trim())
    .find((line) => line.startsWith('更新频率：'))
    ?.replace('更新频率：', '')
    .trim();
  return {
    id,
    title: clean(r.rankname),
    subtitle: subtitle || r.update_frequency || undefined,
    cover: cover(r.imgurl || r.bannerurl),
  };
}

function detectFormat(ext?: string, url?: string): Format | undefined {
  const lower = (ext || url?.split('?')[0].split('.').pop() || '').toLowerCase();
  if (lower === 'mp3' || lower === 'flac' || lower === 'm4a' || lower === 'ogg' || lower === 'opus') {
    return lower;
  }
  return undefined;
}

async function fetchPrivilege(ctx: Context, hash: string): Promise<PrivilegeItem | null> {
  const cleanHash = hash.trim().toLowerCase();
  if (!cleanHash) return null;
  const resp = await kugou<Envelope<PrivilegeItem[]>>(ctx, {
    url: '/v2/get_res_privilege/lite',
    method: 'POST',
    headers: { 'x-router': 'media.store.kugou.com' },
    data: {
      appid: APPID,
      area_code: 1,
      behavior: 'play',
      clientver: CLIENTVER,
      need_hash_offset: 1,
      relate: 1,
      support_verify: 1,
      resource: [{ type: 'audio', page_id: 0, hash: cleanHash, album_id: 0 }],
      qualities: UPSTREAM_QUALITIES,
    },
  });
  const list = Array.isArray(resp.data) ? resp.data : [];
  return list.find((item) => item.hash?.toLowerCase() === cleanHash) || list[0] || null;
}

async function fetchTrackStreams(
  id: string,
  ctx: Context & { quality: Quality; format?: Format },
): Promise<Stream[]> {
  const baseHash = id.trim().toLowerCase();
  if (!baseHash) throw new NotFound();

  const c = credential(ctx);
  const isAuth = Boolean(c.userid && c.token);
  const targetIdx = Math.max(0, QUALITIES.indexOf(ctx.quality || 'high'));
  const desiredQualities = QUALITIES.slice(targetIdx);

  let priv: PrivilegeItem | null = null;
  try {
    priv = await fetchPrivilege(ctx, baseHash);
  } catch (error) {
    if (error instanceof AuthExpired || (error as Error)?.name === 'AbortError') throw error;
  }

  const albumId = Number(priv?.album_id || 0);
  const albumAudioId = Number(priv?.album_audio_id || 0);
  const relate = Array.isArray(priv?.relate_goods) ? priv.relate_goods : [];

  interface Candidate {
    hash: string;
    upstreamQuality: string;
    quality: Quality;
    format?: Format;
    size?: number;
    bitrate?: number;
  }

  const candidates: Candidate[] = [];
  for (const q of desiredQualities) {
    const uq = QUALITY_TO_UPSTREAM[q];
    const match = relate.find((g) => g.quality === uq && g.hash) ||
      (priv?.quality === uq && priv?.hash ? priv : undefined);
    const oggHash =
      q === 'standard'
        ? priv?.trans_param?.ogg_128_hash
        : q === 'high'
          ? priv?.trans_param?.ogg_320_hash
          : undefined;
    const oggSize =
      q === 'standard'
        ? priv?.trans_param?.ogg_128_filesize
        : q === 'high'
          ? priv?.trans_param?.ogg_320_filesize
          : undefined;

    if (match?.hash) {
      const wantOgg = ctx.format === 'ogg' && oggHash;
      candidates.push({
        hash: (wantOgg ? oggHash : match.hash).toLowerCase(),
        upstreamQuality: uq,
        quality: q,
        format: wantOgg ? 'ogg' : detectFormat(match.info?.extname),
        size: wantOgg ? oggSize : match.info?.filesize,
        bitrate: match.info?.bitrate,
      });
    } else if (oggHash) {
      candidates.push({
        hash: oggHash.toLowerCase(),
        upstreamQuality: uq,
        quality: q,
        format: 'ogg',
        size: oggSize,
      });
    }
  }
  if (!candidates.length) {
    candidates.push({
      hash: baseHash,
      upstreamQuality: QUALITY_TO_UPSTREAM[ctx.quality || 'high'] || '128',
      quality: ctx.quality || 'standard',
    });
  }

  const streams: Stream[] = [];
  let sawStatus2 = false;
  let sawNoCopyright = false;

  for (const cand of candidates) {
    const resp = await kugou<SongUrlResponse>(ctx, {
      url: '/v5/url',
      method: 'GET',
      headers: { 'x-router': 'trackercdn.kugou.com' },
      encryptKey: true,
      raw: true,
      params: {
        album_id: albumId,
        album_audio_id: albumAudioId,
        area_code: 1,
        hash: cand.hash,
        ssa_flag: 'is_fromtrack',
        version: 11430,
        clientver: 11430,
        page_id: 967177915,
        ppage_id: '356753938',
        quality: cand.upstreamQuality,
        behavior: 'play',
        pid: 411,
        cmd: 26,
        pidversion: 3001,
        IsFreePart: isAuth ? 0 : 1,
        cdnBackup: 1,
        module: '',
      },
    });
    if (resp.status === 2) {
      sawStatus2 = true;
      break;
    }
    if (resp.status === 3) {
      sawNoCopyright = true;
      continue;
    }
    if (resp.status !== 1 || resp.extName?.toLowerCase() === 'mp4') continue;
    const urls = [...(resp.url || []), ...(resp.backupUrl || [])].filter(Boolean);
    if (!urls.length) continue;
    const format = detectFormat(resp.extName, urls[0]) || cand.format;
    for (const url of urls) {
      streams.push({
        url,
        format,
        quality: cand.quality,
        bitrate: resp.bitRate || cand.bitrate,
        size: resp.fileSize || cand.size,
      });
    }
    // Stop once we have resolved the highest playable tier and its CDN backups.
    break;
  }

  if (streams.length) return streams;
  if (sawStatus2) throw new AuthExpired('酷狗登录已失效，请重新登录');
  if (sawNoCopyright) throw new GeoRestricted('该歌曲暂无版权');
  if (relate.some((g) => g.privilege && g.privilege !== 0) || (priv?.privilege && priv.privilege !== 0)) {
    throw new VipRequired('该歌曲需要酷狗 VIP 会员');
  }
  throw new Expired('酷狗未返回可播放地址');
}

async function fetchLyrics(id: string, ctx: Context): Promise<Lyrics | null> {
  const hash = id.trim().toLowerCase();
  if (!hash) return null;
  const searchResp = await kugou<{ status?: number; candidates?: LyricCandidate[] }>(ctx, {
    baseURL: 'https://lyrics.kugou.com',
    url: '/v1/search',
    method: 'GET',
    clearDefaultParams: true,
    notSignature: true,
    raw: true,
    params: {
      album_audio_id: 0,
      appid: APPID,
      clientver: CLIENTVER,
      duration: 0,
      hash,
      keyword: '',
      lrctxt: 1,
      man: 'no',
    },
  });
  const candidate = searchResp.candidates?.[0];
  if (!candidate?.id || !candidate.accesskey) return null;

  const dl = await kugou<{ status?: number; content?: string; contenttype?: number | string }>(ctx, {
    baseURL: 'https://lyrics.kugou.com',
    url: '/download',
    method: 'GET',
    raw: true,
    params: {
      ver: 1,
      client: 'android',
      id: candidate.id,
      accesskey: candidate.accesskey,
      fmt: 'krc',
      charset: 'utf8',
    },
  });
  if (!dl.content) return null;
  // contenttype === 0 indicates XOR+zlib KRC container; otherwise plain base64 LRC.
  if (Number(dl.contenttype ?? 0) === 0) {
    const text = decodeKrc(dl.content);
    return text ? { format: 'krc', text } : null;
  }
  const text = Buffer.from(dl.content, 'base64').toString('utf8');
  return text ? { format: 'lrc', text } : null;
}

async function searchUpstream<R, T>(
  type: 'song' | 'album' | 'author' | 'special',
  map: (item: R) => T | null,
  args: PageArgs & { keyword: string },
  ctx: Context,
): Promise<Page<T>> {
  const page = parsePage(args);
  const path = type === 'song' ? '/v2/search/song' : `/v1/search/${type}`;
  const params: Record<string, string | number | boolean | undefined> =
    type === 'song'
      ? {
          keyword: args.keyword,
          page,
          pagesize: args.limit,
          platform: 'AndroidFilter',
          iscorrection: 1,
          privilegefilter: 0,
          area_code: 1,
          dopicfull: 1,
          clientver: 201, // Lite (Young) search versionCode
        }
      : type === 'album'
        ? {
            keyword: args.keyword,
            page,
            pagesize: args.limit,
            platform: 'AndroidFilter',
            iscorrection: 1,
            category: '1',
            sorttype: 0,
            searchsong: 0,
            clientver: 201,
          }
        : {
            albumhide: 0,
            iscorrection: 1,
            keyword: args.keyword,
            nocollect: 0,
            page,
            pagesize: args.limit,
            platform: 'AndroidFilter',
          };

  const resp = await kugou<Envelope<{ lists?: R[]; total?: number }>>(ctx, {
    url: path,
    method: 'GET',
    params,
    headers: { 'x-router': 'complexsearch.kugou.com' },
  });
  const items = (resp.data?.lists || []).map(map).filter((v): v is T => v !== null);
  return paged(items, page, args.limit, resp.data?.total);
}

async function fetchSpecialPlaylists(
  ctx: Context,
  categoryId: number,
  page: number,
  limit: number,
  sort = 1,
): Promise<Page<Playlist>> {
  const dev = await ensureDevice(ctx);
  const c = credential(ctx);
  const nowSec = Math.floor(Date.now() / 1000).toString();
  const resp = await kugou<Envelope<{ special_list?: SearchPlaylist[] }>>(ctx, {
    url: '/v2/special_recommend',
    method: 'POST',
    headers: { 'x-router': 'specialrec.service.kugou.com' },
    data: {
      appid: APPID,
      mid: dev.mid,
      clientver: CLIENTVER,
      platform: 'android',
      clienttime: nowSec,
      userid: c.userid,
      module_id: 1,
      page,
      pagesize: limit,
      key: signParamsKey(nowSec),
      special_recommend: {
        withtag: 1,
        withsong: 0,
        sort,
        ugc: 1,
        is_selected: 0,
        withrecommend: 1,
        area_code: 1,
        categoryid: categoryId,
      },
      req_multi: 1,
      retrun_min: 5,
      return_special_falg: 1,
    },
  });
  const items = (resp.data?.special_list || [])
    .map((p) => mapPlaylist(p))
    .filter((v): v is Playlist => v !== null);
  return paged(items, page, limit);
}

async function fetchPlaylistTracksPage(
  id: string,
  page: number,
  limit: number,
  ctx: Context,
): Promise<{ listInfo?: SearchPlaylist; tracks: Page<Track> }> {
  const resp = await kugou<Envelope<{ songs?: PlaylistTrack[]; list_info?: SearchPlaylist }>>(ctx, {
    url: '/pubsongs/v2/get_other_list_file_nofilt',
    method: 'GET',
    params: {
      area_code: 1,
      begin_idx: (page - 1) * limit,
      plat: 1,
      type: 1,
      mode: 1,
      personal_switch: 1,
      extend_fields: 'abtags,hot_cmt,popularization',
      pagesize: limit,
      global_collection_id: id,
    },
  });
  const items = (resp.data?.songs || [])
    .map(mapPlaylistTrack)
    .filter((v): v is Track => v !== null);
  const total = resp.data?.list_info?.count ?? resp.data?.list_info?.song_count;
  return {
    listInfo: resp.data?.list_info,
    tracks: paged(items, page, limit, total),
  };
}

async function fetchYouthCard(
  ctx: Context,
  cardId: number,
  pagesize = 18,
  tagid?: string,
): Promise<YouthCardData | null> {
  const endpoint = cardId === 3103 && !tagid ? 'tag_card_recommend' : 'single_card_recommend';
  const resp = await kugou<Envelope<YouthCardData>>(ctx, {
    url: `/youth/v1/song/${endpoint}`,
    method: 'POST',
    params: {
      card_id: cardId,
      area_code: 1,
      platform: 'ios',
      module_id: 1,
      ver: 'v2',
      pagesize,
      clientver: 12500,
    },
    data: { tagid: tagid ?? '', u_info: '', source_mixsong: '' },
  });
  return resp.data || null;
}

async function fetchYouthChannels(ctx: Context, page = 1, pagesize = 12): Promise<Playlist[]> {
  const resp = await kugou<
    Envelope<YouthChannel[] | { list?: YouthChannel[]; channel_list?: YouthChannel[]; info?: YouthChannel[] }>
  >(ctx, {
    url: '/youth/v2/channel/channel_all_list',
    method: 'GET',
    params: { page, pagesize, type: 1 },
  });
  const data = resp.data;
  const raw = Array.isArray(data)
    ? data
    : data?.list || data?.channel_list || data?.info || [];
  return raw.map(mapYouthChannel).filter((v): v is Playlist => v !== null);
}

async function fetchHomeDiscoverTracks(ctx: Context, page: number, pagesize = 12): Promise<Track[]> {
  const c = credential(ctx);
  const resp = await kugou<Envelope<{ items?: HomeDiscoverItem[] }>>(ctx, {
    url: '/homediscoverrec/v1/client/home_discover_rec',
    method: 'POST',
    params: {
      module_key: 'home_discover_rec',
      module_id: 1,
      area_code: 1,
      platform: 'android',
      userid: c.userid,
      // Upstream requires clientver >= 20489; EchoMusic & KuGouMusicApi use 20809.
      clientver: 20809,
    },
    data: {
      support: 'only_song',
      userid: c.userid,
      recall_type: 'song',
      today_play_num: Math.max(0, (page - 2) * pagesize),
      pagesize,
      go_ky_extra: [
        { key: 'network', val: '2' },
        { key: 'play_mode', val: '2' },
        { key: 'no_mv_ret', val: '1' },
      ],
    },
  });
  return (resp.data?.items || [])
    .filter((item) => item.item_type === 'song' && item.song_info)
    .map((item) => mapRecommendSong(item.song_info!))
    .filter((v): v is Track => v !== null);
}

async function fetchUserPlaylistsRaw(ctx: Context, page: number, limit: number): Promise<SearchPlaylist[]> {
  const c = credential(ctx, true);
  const resp = await kugou<Envelope<{ info?: SearchPlaylist[] }>>(ctx, {
    url: '/v7/get_all_list',
    method: 'POST',
    headers: { 'x-router': 'cloudlist.service.kugou.com' },
    params: { plat: 1, userid: c.userid, token: c.token },
    data: {
      userid: c.userid,
      token: c.token,
      total_ver: 979,
      type: 2,
      page,
      pagesize: limit,
    },
  });
  return (resp.data?.info || []).slice().sort((a, b) => (a.sort ?? 0) - (b.sort ?? 0));
}

export default definePlugin({
  id: 'kugou.music',
  name: '酷狗音乐',
  version: '1.0.0',
  author: 'Haiku',
  description: '酷狗音乐（概念版协议）',
  logo: 'https://www.kugou.com/yy/static/images/play/logo.png',
  covers: { sizes: [120, 150, 240, 400, 480] },
  auth,
  search: {
    track: (args, ctx) => searchUpstream('song', mapSearchTrack, args, ctx),
    album: (args, ctx) => searchUpstream('album', mapAlbum, args, ctx),
    artist: (args, ctx) => searchUpstream('author', mapArtist, args, ctx),
    playlist: (args, ctx) => searchUpstream('special', (p: SearchPlaylist) => mapPlaylist(p), args, ctx),
  },
  track: {
    async get(id, ctx) {
      const priv = await fetchPrivilege(ctx, id);
      const item = priv && mapPrivilegeTrack(priv);
      if (!item) throw new NotFound();
      return item;
    },
    streams: fetchTrackStreams,
    lyrics: fetchLyrics,
  },
  album: {
    async get(id, ctx): Promise<Detail<Album>> {
      const page = parsePage(ctx);
      const [detailResp, songsResp] = await Promise.all([
        page === 1
          ? kugou<Envelope<SearchAlbum[]>>(ctx, {
              url: '/kmr/v2/albums',
              method: 'POST',
              headers: { 'x-router': 'openapi.kugou.com', 'kg-tid': '255' },
              data: {
                data: [{ album_id: id }],
                is_buy: 0,
                fields:
                  'album_id,album_name,publish_date,sizable_cover,intro,language,is_publish,heat,type,quality,authors,author_name,trans_param',
              },
            })
          : Promise.resolve(null),
        kugou<Envelope<{ songs?: AlbumSong[]; total?: number }>>(ctx, {
          url: '/v1/album_audio/lite',
          method: 'POST',
          headers: { 'x-router': 'openapi.kugou.com', 'kg-tid': '255' },
          data: { album_id: id, is_buy: '', page, pagesize: ctx.limit },
        }),
      ]);
      const items = (songsResp.data?.songs || [])
        .map(mapAlbumSong)
        .filter((v): v is Track => v !== null);
      const total = songsResp.data?.total;
      const rawInfo = detailResp?.data?.[0];
      const info = rawInfo
        ? { ...(mapAlbum(rawInfo) || { id, title: clean(rawInfo.album_name) }), id, trackCount: total }
        : undefined;
      if (page === 1 && !info && !items.length) throw new NotFound();
      return { info, tracks: paged(items, page, ctx.limit, total) };
    },
  },
  artist: {
    async get(id, ctx): Promise<Detail<Artist>> {
      const page = parsePage(ctx);
      const dev = await ensureDevice(ctx);
      const nowSec = Math.floor(Date.now() / 1000);
      const [detailResp, songsResp] = await Promise.all([
        page === 1
          ? kugou<Envelope<RemoteSinger>>(ctx, {
              url: '/kmr/v3/author',
              method: 'POST',
              headers: { 'x-router': 'openapi.kugou.com', 'kg-tid': '36' },
              data: { author_id: id },
            })
          : Promise.resolve(null),
        kugou<Envelope<ArtistSong[]>>(ctx, {
          baseURL: 'https://openapi.kugou.com',
          url: '/kmr/v1/audio_group/author',
          method: 'POST',
          headers: { 'x-router': 'openapi.kugou.com', 'kg-tid': '220' },
          data: {
            appid: APPID,
            clientver: CLIENTVER,
            mid: dev.mid,
            clienttime: nowSec,
            key: signParamsKey(nowSec),
            author_id: id,
            pagesize: ctx.limit,
            page,
            sort: 1, // 1=hot, 2=new
            area_code: 'all',
          },
        }),
      ]);
      const items = (Array.isArray(songsResp.data) ? songsResp.data : [])
        .map(mapArtistSong)
        .filter((v): v is Track => v !== null);
      const raw = detailResp?.data;
      const info = raw ? { ...(mapArtist(raw) || { id, name: id }), id } : undefined;
      if (page === 1 && !info && !items.length) throw new NotFound();
      return { info, tracks: paged(items, page, ctx.limit, raw?.song_count) };
    },
    async tracks(id, ctx) {
      const page = parsePage(ctx);
      const dev = await ensureDevice(ctx);
      const nowSec = Math.floor(Date.now() / 1000);
      const resp = await kugou<Envelope<ArtistSong[]>>(ctx, {
        baseURL: 'https://openapi.kugou.com',
        url: '/kmr/v1/audio_group/author',
        method: 'POST',
        headers: { 'x-router': 'openapi.kugou.com', 'kg-tid': '220' },
        data: {
          appid: APPID,
          clientver: CLIENTVER,
          mid: dev.mid,
          clienttime: nowSec,
          key: signParamsKey(nowSec),
          author_id: id,
          pagesize: ctx.limit,
          page,
          sort: 1,
          area_code: 'all',
        },
      });
      const items = (Array.isArray(resp.data) ? resp.data : [])
        .map(mapArtistSong)
        .filter((v): v is Track => v !== null);
      return paged(items, page, ctx.limit);
    },
    async albums(id, ctx) {
      const page = parsePage(ctx);
      const resp = await kugou<Envelope<SearchAlbum[]>>(ctx, {
        url: '/kmr/v1/author/albums',
        method: 'POST',
        headers: { 'x-router': 'openapi.kugou.com', 'kg-tid': '36' },
        data: {
          author_id: id,
          pagesize: ctx.limit,
          page,
          sort: 3, // 3=hot, 1=new
          category: 1,
          area_code: 'all',
        },
      });
      const items = (Array.isArray(resp.data) ? resp.data : [])
        .map(mapAlbum)
        .filter((v): v is Album => v !== null);
      return paged(items, page, ctx.limit);
    },
  },
  playlist: {
    async get(id, ctx): Promise<Detail<Playlist>> {
      const page = parsePage(ctx);
      if (id.startsWith('ch:')) {
        const gid = id.slice(3);
        const [metaResp, songsResp] = await Promise.all([
          page === 1
            ? kugou<Envelope<YouthChannel[] | { list?: YouthChannel[]; info?: YouthChannel[] }>>(ctx, {
                url: '/youth/api/channel/v1/channel_list_by_id',
                method: 'POST',
                data: { data: [{ global_collection_id: gid }] },
              }).catch(() => null)
            : Promise.resolve(null),
          kugou<Envelope<{ list?: RecommendSong[]; songs?: RecommendSong[]; song_list?: RecommendSong[]; total?: number }>>(
            ctx,
            {
              baseURL: 'https://youth.kugou.com',
              url: '/api/channel/v1/channel_get_song_list',
              method: 'GET',
              params: {
                global_collection_id: gid,
                pagesize: ctx.limit,
                page,
                types: 2,
                is_filter: 0,
                apiver: 3,
              },
            },
          ),
        ]);
        const metaData = metaResp?.data;
        const firstChannel = Array.isArray(metaData)
          ? metaData[0]
          : metaData?.list?.[0] || metaData?.info?.[0];
        const info = firstChannel
          ? { ...(mapYouthChannel(firstChannel) || { id, title: gid }), id }
          : page === 1
            ? { id, title: '概念版频道' }
            : undefined;
        const rawSongs = songsResp.data?.list || songsResp.data?.songs || songsResp.data?.song_list || [];
        const items = rawSongs.map(mapRecommendSong).filter((v): v is Track => v !== null);
        if (page === 1 && !firstChannel && !items.length) throw new NotFound();
        return { info, tracks: paged(items, page, ctx.limit, songsResp.data?.total) };
      }

      const [metaResp, tracksData] = await Promise.all([
        page === 1
          ? kugou<Envelope<SearchPlaylist[]>>(ctx, {
              url: '/v3/get_list_info',
              method: 'POST',
              headers: { 'x-router': 'pubsongs.kugou.com' },
              data: {
                data: [{ global_collection_id: id }],
                userid: credential(ctx).userid,
                token: credential(ctx).token,
              },
            }).catch(() => null)
          : Promise.resolve(null),
        fetchPlaylistTracksPage(id, page, ctx.limit, ctx),
      ]);
      const rawInfo = metaResp?.data?.[0] || tracksData.listInfo;
      const info = rawInfo ? { ...(mapPlaylist(rawInfo) || { id, title: id }), id } : undefined;
      if (page === 1 && !info && !tracksData.tracks.items.length) throw new NotFound();
      return { info, tracks: tracksData.tracks };
    },
  },
  toplist: {
    async list(ctx) {
      const resp = await kugou<Envelope<{ info?: RankItem[] }>>(ctx, {
        url: '/ocean/v6/rank/list',
        method: 'GET',
        params: { plat: 2, withsong: 1, parentid: 0 },
      });
      return (resp.data?.info || []).map(mapToplist).filter((v): v is Toplist => v !== null);
    },
    async get(id, ctx): Promise<Detail<Toplist>> {
      const page = parsePage(ctx);
      const [infoResp, songsResp] = await Promise.all([
        page === 1
          ? kugou<Envelope<RankItem>>(ctx, {
              url: '/ocean/v6/rank/info',
              method: 'GET',
              params: { rank_cid: 0, rankid: id, with_album_img: 1, zone: '' },
            })
          : Promise.resolve(null),
        kugou<Envelope<{ songlist?: RankSong[]; total?: number }>>(ctx, {
          url: '/openapi/kmr/v2/rank/audio',
          method: 'POST',
          headers: { 'kg-tid': '369' },
          data: {
            show_portrait_mv: 1,
            show_type_total: 1,
            filter_original_remarks: 1,
            area_code: 1,
            pagesize: ctx.limit,
            rank_cid: 0,
            type: 1,
            page,
            rank_id: id,
          },
        }),
      ]);
      const items = (songsResp.data?.songlist || [])
        .map(mapRankSong)
        .filter((v): v is Track => v !== null);
      const rawInfo = infoResp?.data;
      const info = rawInfo ? { ...(mapToplist(rawInfo) || { id, title: id }), id } : undefined;
      if (page === 1 && !info && !items.length) throw new NotFound();
      return { info, tracks: paged(items, page, ctx.limit, songsResp.data?.total) };
    },
  },
  async home(ctx) {
    const page = Number(ctx.cursor || 1);
    if (!Number.isSafeInteger(page) || page < 1) throw new ParseError('Invalid home cursor');

    if (page === 1) {
      // Mirrors the Kugou Lite (概念版) mobile discovery feed + EchoMusic Home sections.
      const [
        card3005,
        card3101,
        card3102,
        card3103,
        card3001,
        card3004,
        card3014,
        dailyRes,
        channels,
        styleRes,
        topIpRes,
        playlistPage,
        aiRes,
      ] = await Promise.all([
        fetchYouthCard(ctx, 3005, 18).catch(() => null),
        fetchYouthCard(ctx, 3101, 18).catch(() => null),
        fetchYouthCard(ctx, 3102, 18).catch(() => null),
        fetchYouthCard(ctx, 3103, 6).catch(() => null),
        fetchYouthCard(ctx, 3001, 18).catch(() => null),
        fetchYouthCard(ctx, 3004, 18).catch(() => null),
        fetchYouthCard(ctx, 3014, 18).catch(() => null),
        kugou<Envelope<{ song_list?: RecommendSong[] }>>(ctx, {
          url: '/everyday_song_recommend',
          method: 'POST',
          params: { platform: 'ios' },
          headers: { 'x-router': 'everydayrec.service.kugou.com' },
        }).catch(() => null),
        fetchYouthChannels(ctx, 1, 12).catch(() => [] as Playlist[]),
        kugou<Envelope<{ title?: string; song_list?: RecommendSong[] }>>(ctx, {
          url: '/everydayrec.service/everyday_style_recommend',
          method: 'POST',
          params: { tagids: '' },
          data: {},
        }).catch(() => null),
        kugou<Envelope<{ list?: TopIpItem[] }>>(ctx, {
          baseURL: 'https://musicadservice.kugou.com',
          url: '/v1/daily_recommend',
          method: 'POST',
          params: { clientver: 12349, area_code: 1 },
          data: { tags: {} },
        }).catch(() => null),
        fetchSpecialPlaylists(ctx, 0, 1, 12).catch(() => ({ items: [] as Playlist[] })),
        kugou<Envelope<{ card_info?: { card_title?: string }; list?: RecommendSong[] }>>(ctx, {
          url: '/concepts/v1/ai/recommend_song',
          method: 'POST',
          data: { apiver: 2, source: 2, pagesize: 18, page: 1 },
        }).catch(() => null),
      ]);

      const toTracks = (list?: RecommendSong[]) =>
        (list || []).map(mapRecommendSong).filter((v): v is Track => v !== null);

      const t3005 = toTracks(card3005?.song_list);
      const t3101 = toTracks(card3101?.song_list);
      const t3102 = toTracks(card3102?.song_list);
      const t3001 = toTracks(card3001?.song_list);
      const t3004 = toTracks(card3004?.song_list);
      const t3014 = toTracks(card3014?.song_list);
      const dailyTracks = toTracks(dailyRes?.data?.song_list);
      const styleTracks = toTracks(styleRes?.data?.song_list);
      const aiTracks = toTracks(aiRes?.data?.list);
      const topIpPlaylists = (topIpRes?.data?.list || [])
        .map(mapTopIpPlaylist)
        .filter((v): v is Playlist => v !== null);

      const fallbackCover =
        t3001[0]?.cover || dailyTracks[0]?.cover || t3005[0]?.cover || styleTracks[0]?.cover;

      const entries: Extract<Shelf, { type: 'entries' }>['items'] = [
        {
          title: '红心 Radio',
          subtitle: '根据你的听歌口味推荐',
          cover: t3001[0]?.cover || fallbackCover,
          to: { radio: 'fm' },
        },
        ...(dailyTracks.length
          ? [
              {
                title: '每日推荐',
                subtitle: '每天为你量身定制',
                cover: dailyTracks[0]?.cover || fallbackCover,
                to: { radio: 'daily' },
              },
            ]
          : []),
        ...(card3102?.card_info?.label_name
          ? [
              {
                title: `喜欢「${clean(card3102.card_info.label_name)}」`,
                subtitle: clean(card3102.card_info.label_desc) || '同好风格电台',
                cover: cover(card3102.card_info.cover_pic) || t3102[0]?.cover || fallbackCover,
                to: { radio: 'youth:3102' },
              },
            ]
          : []),
        ...(card3103?.tag_list || []).slice(0, 4).flatMap((tag) => {
          const labelId = String(tag.label_id ?? '');
          const labelName = clean(tag.label_name);
          if (!labelId || !labelName) return [];
          const firstSongCover = toTracks(tag.song_list)[0]?.cover;
          return [
            {
              title: `曲风盲盒 · ${labelName}`,
              subtitle: clean(tag.label_desc) || '随机心动',
              cover: cover(tag.cover_pic || tag.top_cover_pic) || firstSongCover || fallbackCover,
              to: { radio: `youth:3103:${labelId}` },
            },
          ];
        }),
      ];

      const shelves: Shelf[] = [];
      if (entries.length) {
        shelves.push({ id: 'kugou:entries', title: '概念漫游', type: 'entries', items: entries });
      }
      if (t3005.length) {
        shelves.push({
          id: 'kugou:card:3005',
          title: clean(card3005?.card_info?.card_title) || '潮流尝鲜',
          type: 'tracks',
          items: t3005,
        });
      }
      if (channels.length) {
        shelves.push({
          id: 'kugou:channels',
          title: '频道推荐',
          type: 'playlists',
          items: channels,
        });
      }
      if (t3101.length) {
        shelves.push({
          id: 'kugou:card:3101',
          title: clean(card3101?.card_info?.card_title) || '概念er新推',
          type: 'tracks',
          items: t3101,
        });
      }
      if (t3102.length) {
        shelves.push({
          id: 'kugou:card:3102',
          title: clean(card3102?.card_info?.card_title) || '同好风格推荐',
          type: 'tracks',
          items: t3102,
        });
      }
      if (t3001.length) {
        shelves.push({
          id: 'kugou:card:3001',
          title: clean(card3001?.card_info?.card_title) || '私人专属好歌',
          type: 'tracks',
          items: t3001,
        });
      }
      if (t3004.length) {
        shelves.push({
          id: 'kugou:card:3004',
          title: clean(card3004?.card_info?.card_title) || '小众宝藏佳作',
          type: 'tracks',
          items: t3004,
        });
      }
      if (t3014.length) {
        shelves.push({
          id: 'kugou:card:3014',
          title: clean(card3014?.card_info?.card_title) || '喜欢这首歌的TA也喜欢',
          type: 'tracks',
          items: t3014,
        });
      }
      if (dailyTracks.length) {
        shelves.push({ id: 'kugou:daily', title: '每日推荐', type: 'tracks', items: dailyTracks });
      }
      if (styleTracks.length) {
        const styleName = clean(styleRes?.data?.title);
        shelves.push({
          id: 'kugou:style',
          title: styleName ? `风格推荐 · ${styleName}` : '风格推荐',
          type: 'tracks',
          items: styleTracks,
        });
      }
      if (topIpPlaylists.length) {
        shelves.push({
          id: 'kugou:top-ip',
          title: '编辑精选',
          type: 'playlists',
          items: topIpPlaylists,
        });
      }
      if (playlistPage.items.length) {
        shelves.push({
          id: 'kugou:playlists',
          title: '推荐歌单',
          type: 'playlists',
          items: playlistPage.items,
          more: { category: '0' },
        });
      }
      if (aiTracks.length) {
        shelves.push({
          id: 'kugou:ai-free',
          title: clean(aiRes?.data?.card_info?.card_title) || '免费听精选',
          type: 'tracks',
          items: aiTracks,
        });
      }
      return { shelves, next: '2' };
    }

    // Endless waterfall feed below the lead shelves: EchoMusic home_discover + recommended playlists.
    const [discoverTracks, morePlaylists] = await Promise.all([
      fetchHomeDiscoverTracks(ctx, page, 12).catch(() => [] as Track[]),
      fetchSpecialPlaylists(ctx, 0, page, 8).catch(() => ({ items: [] as Playlist[], next: undefined })),
    ]);
    const shelves: Shelf[] = [];
    if (discoverTracks.length) {
      shelves.push({
        id: `kugou:feed:tracks:p${page}`,
        title: '为你推荐',
        type: 'tracks',
        layout: 'feed',
        items: discoverTracks,
      });
    }
    if (morePlaylists.items.length) {
      shelves.push({
        id: `kugou:feed:playlists:p${page}`,
        title: '为你推荐',
        type: 'playlists',
        layout: 'feed',
        items: morePlaylists.items,
      });
    }
    const hasNext = page < 20 && (discoverTracks.length > 0 || Boolean(morePlaylists.next));
    return { shelves, next: hasNext ? String(page + 1) : undefined };
  },
  categories: {
    async list(ctx) {
      const resp = await kugou<Envelope<PlaylistTagGroup[]>>(ctx, {
        url: '/pubsongs/v1/get_tags_by_type',
        method: 'POST',
        data: { tag_type: 'collection', tag_id: 0, source: 3 },
      });
      const groups = (Array.isArray(resp.data) ? resp.data : [])
        .map((g) => ({
          title: clean(g.tag_name),
          items: (g.son || [])
            .map((sub) => ({
              id: String(sub.tag_id ?? ''),
              title: clean(sub.tag_name),
              cover: cover(sub.img),
            }))
            .filter((item) => item.id && item.title),
        }))
        .filter((g) => g.title && g.items.length);
      return {
        sorts: [
          { id: '1', title: '推荐' },
          { id: '2', title: '最热' },
          { id: '3', title: '最新' },
        ],
        groups,
      };
    },
    async playlists(id, ctx) {
      const page = parsePage(ctx);
      const sort = Number(ctx.sort || 1);
      if (![1, 2, 3].includes(sort)) throw new Unsupported(`Unknown Kugou playlist sort: ${ctx.sort}`);
      return fetchSpecialPlaylists(ctx, Number(id) || 0, page, ctx.limit, sort);
    },
  },
  me: {
    async playlists(ctx) {
      const c = credential(ctx, true);
      const page = parsePage(ctx);
      const raw = await fetchUserPlaylistsRaw(ctx, page, ctx.limit);
      // Match MoeKoeMusic Library.vue: filter out collected albums (non-owned entries with authors).
      const items = raw
        .filter((p) => p.list_create_userid === c.userid || p.name === '我喜欢' || !p.authors?.length)
        .map((p) => mapPlaylist(p, p.list_create_userid === c.userid || p.name === '我喜欢'))
        .filter((v): v is Playlist => v !== null);
      return paged(items, page, ctx.limit);
    },
    async likedTracks(ctx) {
      credential(ctx, true);
      let gid = '';
      let page = 1;
      if (ctx.cursor) {
        const at = ctx.cursor.lastIndexOf(':');
        if (at < 1) throw new ParseError('Invalid likedTracks cursor');
        gid = ctx.cursor.slice(0, at);
        page = parsePage({ cursor: ctx.cursor.slice(at + 1), limit: ctx.limit });
      } else {
        const lists = await fetchUserPlaylistsRaw(ctx, 1, 100);
        const liked = lists.find((p) => p.name === '我喜欢' || Number(p.listid) === 2);
        gid = String(liked?.global_collection_id || '');
        if (!gid) return { items: [] };
      }
      const res = await fetchPlaylistTracksPage(gid, page, ctx.limit, ctx);
      return {
        items: res.tracks.items,
        total: res.tracks.total,
        next: res.tracks.next ? `${gid}:${res.tracks.next}` : undefined,
      };
    },
  },
  radio: {
    async channels(ctx) {
      const base = YOUTH_RADIO_CHANNELS.map((c) => ({ id: c.id, title: c.title }));
      try {
        const dev = await ensureDevice(ctx);
        const c = credential(ctx);
        const nowMs = Date.now();
        const resp = await kugou<
          Envelope<{
            data?: { fmid?: number | string; fmname?: string; imgurl?: string; fm_list?: { fmid?: number | string; fmname?: string; imgurl?: string }[] }[];
          }>
        >(ctx, {
          url: '/v1/class_fm_song',
          method: 'POST',
          headers: { 'x-router': 'fm.service.kugou.com' },
          data: {
            kguid: c.userid,
            clienttime: nowMs,
            mid: dev.mid,
            platform: 'android',
            clientver: CLIENTVER,
            uid: c.userid,
            get_tracker: 1,
            key: signParamsKey(nowMs),
            appid: APPID,
          },
        });
        const extra = (resp.data?.data || [])
          .flatMap((g) => (g.fm_list ? g.fm_list : [g]))
          .map((fm) => ({
            id: fm.fmid ? `fm:${fm.fmid}` : '',
            title: clean(fm.fmname),
            cover: cover(fm.imgurl),
          }))
          .filter((ch) => ch.id && ch.title);
        return [...base, ...extra];
      } catch (error) {
        if ((error as Error)?.name === 'AbortError') throw error;
        return base;
      }
    },
    async next(channelId, ctx) {
      const dev = await ensureDevice(ctx);
      const c = credential(ctx);
      const nowMs = Date.now();
      const seen = new Set(ctx.history.map((t) => t.id.toLowerCase()));
      let songs: RecommendSong[] = [];

      if (channelId === undefined || channelId === 'fm') {
        // Private FM via persnfm.service.kugou.com/v2/personal_recommend.
        const last = ctx.history[ctx.history.length - 1];
        const resp = await kugou<Envelope<{ song_list?: RecommendSong[]; list?: RecommendSong[] }>>(
          ctx,
          {
            url: '/v2/personal_recommend',
            method: 'POST',
            headers: { 'x-router': 'persnfm.service.kugou.com' },
            data: {
              appid: APPID,
              clienttime: nowMs,
              mid: dev.mid,
              action: 'play',
              recommend_source_locked: 0,
              song_pool_id: 0,
              callerid: 0,
              m_type: 1,
              platform: 'ios',
              area_code: 1,
              remain_songcnt: 0,
              clientver: CLIENTVER,
              is_overplay: 0,
              mode: 'normal',
              fakem: 'ca981cfc583a4c37f28d2d49000013c16a0a',
              key: signParamsKey(nowMs),
              ...(c.userid ? { userid: c.userid, kguid: c.userid } : {}),
              ...(c.token ? { token: c.token } : {}),
              ...(c.vip_type ? { vip_type: c.vip_type } : {}),
              ...(last ? { hash: last.id } : {}),
            },
          },
        );
        songs = resp.data?.song_list || resp.data?.list || [];
      } else if (channelId === 'daily') {
        const resp = await kugou<Envelope<{ song_list?: RecommendSong[] }>>(ctx, {
          url: '/everyday_song_recommend',
          method: 'POST',
          params: { platform: 'ios' },
          headers: { 'x-router': 'everydayrec.service.kugou.com' },
        });
        songs = resp.data?.song_list || [];
      } else if (channelId.startsWith('youth:')) {
        const parts = channelId.split(':');
        const cardId = Number(parts[1]);
        const tagId = parts[2];
        if (!Number.isSafeInteger(cardId) || cardId <= 0) {
          throw new Unsupported(`Unknown Kugou Youth radio channel: ${channelId}`);
        }
        const data = await fetchYouthCard(ctx, cardId, Math.max(ctx.count * 2, 15), tagId);
        songs = data?.song_list || (data?.tag_list || []).flatMap((t) => t.song_list || []);
      } else if (LEGACY_CARD_IDS.includes(channelId)) {
        const fakem = '60f7ebf1f812edbac3c63a7310001701760f';
        const resp = await kugou<Envelope<{ song_list?: RecommendSong[] }>>(ctx, {
          url: '/singlecardrec.service/v1/single_card_recommend',
          method: 'POST',
          params: { card_id: channelId, fakem, area_code: 1, platform: 'ios' },
          data: {
            appid: APPID,
            clientver: CLIENTVER,
            platform: 'android',
            clienttime: nowMs,
            userid: c.userid,
            key: signParamsKey(nowMs),
            fakem,
            area_code: 1,
            mid: dev.mid,
            uuid: '-',
            client_playlist: [],
            u_info: 'a0c35cd40af564444b5584c2754dedec',
          },
        });
        songs = resp.data?.song_list || [];
      } else if (channelId.startsWith('fm:')) {
        const fmid = channelId.slice(3);
        const resp = await kugou<Envelope<{ data?: { songs?: RecommendSong[] }[] }>>(ctx, {
          url: '/v1/app_song_list_offset',
          method: 'POST',
          headers: { 'x-router': 'fm.service.kugou.com' },
          data: {
            appid: APPID,
            area_code: 1,
            clienttime: nowMs,
            clientver: CLIENTVER,
            data: [{ fmid, fmtype: 2, offset: -1, size: Math.max(ctx.count, 10), singername: '' }],
            get_tracker: 1,
            key: signParamsKey(nowMs),
            mid: dev.mid,
            uid: c.userid,
          },
        });
        songs = (resp.data?.data || []).flatMap((item) => item.songs || []);
      } else {
        throw new Unsupported(`Unknown Kugou radio channel: ${channelId}`);
      }

      return songs
        .map(mapRecommendSong)
        .filter((t): t is Track => t !== null && !seen.has(t.id))
        .slice(0, ctx.count);
    },
  },
  scrape: {
    async search(query, ctx) {
      const keyword = [query.title, ...query.artists].filter(Boolean).join(' ');
      const res = await searchUpstream('song', mapSearchTrack, { keyword, limit: 10 }, ctx);
      return res.items.map((t) => ({
        id: t.id,
        title: t.title,
        artists: t.artists.map((a) => (typeof a === 'string' ? a : a.name)),
        album: t.album?.title,
        cover: t.cover,
        duration: t.duration,
        extra: t.extra,
      }));
    },
    lyrics: (candidate, ctx) => fetchLyrics(candidate.id, ctx),
  },
});
