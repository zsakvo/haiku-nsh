// Protocol mapping from the reference qq.music plugin (QQMusicApi 27861e5).
// QRC decoder below retains its original GPL-3.0 attribution.
import { randomBytes, createHash, createCipheriv, publicEncrypt } from 'crypto';
import {
  AuthRequired,
  AuthExpired,
  VipRequired,
  NotFound,
  RateLimited,
  Upstream,
  ParseError,
  Unsupported,
} from 'haiku';
import { Buffer } from 'buffer';
import { inflateSync } from 'zlib';

let identityFlight;
const songs = new Map();

export function createProtocol(ctx) {
  const PROVIDER = 'qq';
  const URL_MUSICU = 'https://u.y.qq.com/cgi-bin/musicu.fcg';
  const URL_MUSICS = 'https://u.y.qq.com/cgi-bin/musics.fcg';
  const QIMEI_APP_VERSION = '20.9.0.8';
  const QIMEI_SDK_VERSION = '1.2.13.6';
  const QIMEI_APP_KEY = '0AND0HD6FE4HY80F';
  const QIMEI_SECRET = 'ZdJqM15EeO2zWc08';
  const CHANNEL_ID = '10003505';
  const PACKAGE_ID = 'com.tencent.qqmusic';
  const HEX_CHARS = '0123456789abcdef';
  const WEB_UA =
    'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36';
  const WEB_HEADERS = { Referer: 'https://y.qq.com/', 'User-Agent': WEB_UA };
  const utf8 = (s) => new Uint8Array(Buffer.from(s, 'utf8'));
  const toB64 = (bytes) => Buffer.from(bytes).toString('base64');
  const nowSecs = () => Math.floor(Date.now() / 1000);
  const randHex = (n) =>
    Buffer.from(randomBytes(Math.ceil(n / 2)))
      .toString('hex')
      .slice(0, n);
  const randInt = (min, max) => min + Number(bytesToBigInt(randomBytes(4)) % BigInt(max - min + 1));
  const randChoice = (pool, count) =>
    Array.from(randomBytes(count), (b) => pool[b % pool.length]).join('');
  const getJson = (key) => ctx.storage.get(key);
  const setJson = (key, value) => ctx.storage.set(key, value);
  let currentCredential = ctx.credential || {};
  const getCredential = () => currentCredential;
  const setCredential = (value) => {
    currentCredential = value;
  };

  function musicError(code, message, extra = {}) {
    const errors = {
      AUTH_REQUIRED: AuthRequired,
      AUTH_EXPIRED: AuthExpired,
      VIP_REQUIRED: VipRequired,
      NOT_FOUND: NotFound,
      RATE_LIMITED: RateLimited,
      UPSTREAM_ERROR: Upstream,
      PARSE_ERROR: ParseError,
      BAD_ARGS: ParseError,
      LOGIN_FAILED: AuthRequired,
      CAPABILITY_UNSUPPORTED: Unsupported,
    };
    return new (errors[code] || Upstream)({ message, ...extra });
  }

  function hash33(value, hash = 0) {
    for (let i = 0; i < value.length; i++) hash = (hash * 33 + value.charCodeAt(i)) % 2147483648;
    return hash;
  }

  function credentialLoginType(cred) {
    return cred.login_type ?? (cred.musickey ? (cred.musickey.startsWith('W_X') ? 1 : 2) : 0);
  }

  function credentialCookies(cred) {
    if (!cred.musickey) return '';
    const uin = cred.str_musicid || String(cred.musicid);
    return cookieHeader({
      uin,
      qqmusic_uin: uin,
      qm_keyst: cred.musickey,
      qqmusic_key: cred.musickey,
    });
  }

  async function http(url, options = {}) {
    try {
      const response = await fetch(url, { ...options, signal: ctx.signal });
      if (!response.ok && options.redirect !== 'manual') {
        if (response.status === 401) throw new AuthExpired();
        if (response.status === 429) throw new RateLimited();
        throw musicError('UPSTREAM_ERROR', `HTTP ${response.status}`, {
          retriable: response.status >= 500,
        });
      }
      return response;
    } catch (error) {
      if (error.name === 'AbortError' || error.code) throw error;
      throw musicError('UPSTREAM_ERROR', error.message, { retriable: true });
    }
  }

  async function jsonRequest(url, options = {}) {
    const response = await http(url, options);
    try {
      return await response.json();
    } catch {
      throw musicError('PARSE_ERROR', 'QQ 音乐返回了无效 JSON');
    }
  }

  function checkCode(code, endpoint) {
    if (code === 0) return;
    const kind = [1000, 104400, 104401].includes(code)
      ? 'AUTH_REQUIRED'
      : [2001, 104604].includes(code)
        ? 'RATE_LIMITED'
        : 'UPSTREAM_ERROR';
    throw musicError(kind, `${endpoint}: code=${code}`, {
      upstreamCode: code,
      retriable: kind === 'RATE_LIMITED',
    });
  }

  function buildComm(platform, cred, device, qimei, session) {
    const guid = device.openUdid;
    const values =
      platform === 'web'
        ? {
            ct: 24,
            cv: 4747474,
            platform: 'yqq.json',
            chid: 0,
            uin: cred.musicid || 0,
            g_tk: hash33(cred.musickey || '', 5381),
            g_tk_new_20200303: hash33(cred.musickey || '', 5381),
            format: 'json',
            inCharset: 'utf-8',
            outCharset: 'utf-8',
            notice: 0,
            need_new_code: 1,
          }
        : {
            ct: 11,
            cv: 20090008,
            v: 20090008,
            chid: CHANNEL_ID,
            tmeAppID: 'qqmusic',
            ...(cred.musickey
              ? { qq: cred.musicid, authst: cred.musickey, tmeLoginType: credentialLoginType(cred) }
              : {}),
            traceid: `10002_${cred.musicid || guid}_${nowSecs()}`,
            QIMEI36: qimei.q36,
            OpenUDID: guid,
            udid: guid,
            OpenUDID2: device.openUdid2,
            ...(session ? { uid: session.uid, sid: session.sid } : {}),
            aid: device.androidId,
            os_ver: device.release,
            phonetype: device.model
              .replace(/&/g, '&amp;')
              .replace(/</g, '&lt;')
              .replace(/>/g, '&gt;')
              .replace(/"/g, '&quot;'),
          };
    return Object.fromEntries(Object.entries(values).map(([key, value]) => [key, String(value)]));
  }

  function userAgent(platform, device) {
    return platform === 'web' ? WEB_UA : `QQMusic 20090008(android ${device.release})`;
  }

  async function androidIdentity() {
    const qimei = getJson('qimei');
    const session = getJson('session');
    const today = new Date().toDateString();
    if (
      qimei?.q36 &&
      nowSecs() - qimei.savedAt < 86400 &&
      session?.sid &&
      new Date(session.savedAt * 1000).toDateString() === today
    ) {
      return { qimei, session };
    }
    if (!identityFlight)
      identityFlight = createIdentity().finally(() => {
        identityFlight = undefined;
      });
    return identityFlight;
  }

  async function createIdentity() {
    const device = getDevice();
    let qimei = getJson('qimei');
    if (!qimei?.q36 || nowSecs() - qimei.savedAt >= 86400) {
      const keyText = randChoice(HEX_CHARS, 16);
      const key = toB64(rsaEncryptPkcs1(utf8(keyText)));
      const params = toB64(
        aesCbcEncrypt(utf8(keyText), utf8(JSON.stringify(randomQimeiPayload(device)))),
      );
      const nonce = randChoice(HEX_CHARS, 16);
      const time = nowSecs();
      const extra = `{"appKey":"${QIMEI_APP_KEY}"}`;
      const body = await jsonRequest('https://api.tencentmusic.com/tme/trpc/proxy', {
        method: 'POST',
        headers: {
          method: 'GetQimei',
          service: 'trpc.tme_datasvr.qimeiproxy.QimeiProxy',
          appid: 'qimei_qq_android',
          sign: calcMd5('qimei_qq_androidpzAuCmaFAaFaHrdakPjLIEqKrGnSOOvH', String(time)),
          'User-Agent': 'QQMusic',
          timestamp: String(time),
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          app: 0,
          os: 1,
          qimeiParams: {
            key,
            params,
            time: String(time),
            nonce,
            extra,
            sign: calcMd5(key, params, String(time * 1000), nonce, QIMEI_SECRET, extra),
          },
        }),
      });
      qimei = JSON.parse(body.data || '{}').data;
      if (!qimei?.q16 || !qimei?.q36) throw musicError('UPSTREAM_ERROR', 'QIMEI 响应缺少设备标识');
      qimei = { q16: qimei.q16, q36: qimei.q36, savedAt: nowSecs() };
      setJson('qimei', qimei);
    }
    let session = getJson('session');
    if (
      !session?.sid ||
      new Date(session.savedAt * 1000).toDateString() !== new Date().toDateString()
    ) {
      const [item] = await sendCgi(
        [
          {
            module: 'music.getSession.session',
            method: 'GetSession',
            param: { uid: session?.uid || '', vkey: 0, caller: session?.uid ? 1 : 2 },
          },
        ],
        buildComm('android', {}, device, qimei, null),
        'android',
        {},
      );
      checkCode(item.code, 'GetSession');
      if (!item.data.session?.uid || !item.data.session?.sid)
        throw musicError('PARSE_ERROR', 'GetSession 缺少 uid/sid');
      session = { ...item.data.session, savedAt: nowSecs() };
      setJson('session', session);
    }
    return { qimei, session };
  }

  async function sendCgi(requests, comm, platform, credential, sign = false) {
    const payload = { comm };
    requests.forEach((req, index) => {
      payload[`req_${index}`] = {
        module: req.module,
        method: req.method,
        param: req.preserveBool ? req.param : boolToInt(req.param || {}),
      };
    });
    const body = JSON.stringify(payload);
    const url = sign ? `${URL_MUSICS}?_=${Date.now()}&sign=${zzcSign(body)}` : URL_MUSICU;
    const cookie = credentialCookies(credential);
    const response = await jsonRequest(url, {
      method: 'POST',
      body,
      headers: {
        ...WEB_HEADERS,
        'User-Agent': userAgent(platform, getDevice()),
        'Content-Type': 'application/json',
        ...(cookie ? { Cookie: cookie } : {}),
      },
    });
    checkCode(response.code ?? 0, 'musicu');
    return requests.map((req, index) => {
      const item = response[`req_${index}`];
      if (!item) throw musicError('PARSE_ERROR', `${req.method}: 缺少 req_${index}`);
      return item;
    });
  }

  // Batch only endpoints with the same platform, credentials and comm.
  async function musicBatch(requests, options = {}, refreshed = false) {
    const platform = options.platform || 'android';
    const cred = getCredential();
    const identity = platform === 'android' ? await androidIdentity() : {};
    const comm = { ...buildComm(platform, cred, getDevice(), identity.qimei, identity.session) };
    for (const [key, value] of Object.entries(options.comm || {})) comm[key] = String(value);
    const items = await sendCgi(requests, comm, platform, cred, options.sign);
    if (!options.allowError && items.some((item) => [1000, 104400, 104401].includes(item.code))) {
      throw new AuthExpired('QQ 音乐凭据已过期');
    }
    return items.map((item, index) => {
      if (options.allowError) return item;
      checkCode(item.code, requests[index].method);
      return item.data;
    });
  }

  async function musicRequest(req) {
    const [data] = await musicBatch([req], req);
    return data;
  }

  let credentialFlight;
  async function refreshCredential() {
    if (!credentialFlight) {
      credentialFlight = (async () => {
        const cred = getCredential();
        const type = credentialLoginType(cred);
        const param = {
          openid: cred.openid || '',
          refresh_token: cred.refresh_token || '',
          musickey: cred.musickey,
          refresh_key: cred.refresh_key || '',
          loginMode: 2,
          ...(type !== 2 ? { str_musicid: cred.str_musicid, unionid: cred.unionid || '' } : {}),
          ...(type !== 1
            ? {
                musicid: cred.musicid,
                access_token: cred.access_token || '',
                expired_in: cred.expired_at || 0,
              }
            : {}),
        };
        const item = await musicRequest({
          module: 'music.login.LoginServer',
          method: 'Login',
          comm: { tmeLoginType: type },
          param,
          allowError: true,
        });
        checkCode(item.code, 'Login');
        if (!item.data?.musickey) throw musicError('AUTH_REQUIRED', '凭据续期失败，请重新登录');
        setCredential(normalizeCredential(item.data, cred));
      })().finally(() => {
        credentialFlight = undefined;
      });
    }
    return credentialFlight;
  }

  function normalizeCredential(data, previous = {}) {
    const merged = { ...previous };
    for (const [key, value] of Object.entries(data)) {
      if (value !== '' && value != null) merged[key] = value;
    }
    merged.musickey = data.musickey || data.musicKey || previous.musickey;
    merged.musicid = data.musicid || data.musicId || previous.musicid;
    merged.str_musicid = data.str_musicid || String(merged.musicid);
    merged.login_type =
      data.login_type ??
      data.loginType ??
      previous.login_type ??
      (merged.musickey.startsWith('W_X') ? 1 : 2);
    return merged;
  }

  function requireLogin() {
    const credential = getCredential();
    if (!credential.musickey) throw musicError('AUTH_REQUIRED', '请先登录 QQ 音乐');
    return credential;
  }

  function parseId(value, kind = 'song') {
    const match = /^qq:([^:]+):(.+)$/.exec(String(value || ''));
    return match ? { kind: match[1], id: match[2] } : { kind, id: String(value || '') };
  }

  function songId(args) {
    const { id } = parseId(args.trackId || args.providerId);
    if (!id) throw musicError('NOT_FOUND', '缺少歌曲 ID');
    return id;
  }

  function coverUrl(value, size = 300) {
    if (!value) return undefined;
    const url = String(value).replace(/^\/\//, 'https://');
    const target = [150, 300, 500, 800, 1200, 1500].find((n) => n >= size);
    // Preserve the HTTP image transport used by QQ's own search responses.
    return url
      .replace(
        /^https?:\/\/(?:y\.gtimg\.cn|y\.qq\.com)\/music\/photo_new\//,
        'http://y.gtimg.cn/music/photo_new/',
      )
      .replace(/(\/T00[12])(?:R\d+x\d+)?(M000)/, `$1${target ? `R${target}x${target}` : ''}$2`);
  }

  function photo(mid, kind = 'T002') {
    return mid ? `http://y.gtimg.cn/music/photo_new/${kind}R300x300M000${mid}.jpg` : undefined;
  }

  function track(song) {
    const mid = song.mid || song.songmid;
    if (!mid) return null;
    songs.set(mid, song);
    if (song.id) songs.set(String(song.id), song);
    const album = song.album || {};
    return {
      trackId: `qq:song:${mid}`,
      provider: PROVIDER,
      title: song.title || song.name,
      artists: (song.singer || []).map((singer) => singer.name),
      cover: photo(album.mid || album.pmid) || photo(song.singer?.[0]?.mid, 'T001'),
      album: album.name,
      duration: song.interval,
    };
  }

  function queryTracks(ids) {
    const numeric = ids.every((id) => /^\d+$/.test(id));
    return musicRequest({
      module: 'music.trackInfo.UniformRuleCtrl',
      method: 'CgiGetTrackInfo',
      param: {
        [numeric ? 'ids' : 'mids']: numeric ? ids.map(Number) : ids,
        types: ids.map(() => 0),
        modify_stamp: ids.map(() => 0),
        ctx: 0,
        client: 1,
      },
    }).then((data) => {
      for (const song of data.tracks) track(song);
      return data.tracks;
    });
  }

  async function songInfo(id) {
    if (songs.has(id)) return songs.get(id);
    const [song] = await queryTracks([id]);
    if (!song) throw musicError('NOT_FOUND', '歌曲不存在');
    return song;
  }

  async function getTrack(args) {
    return track(await songInfo(songId(args)));
  }

  async function getCoverArt(args) {
    const cover = args.cover || (await getTrack(args)).cover;
    return {
      provider: PROVIDER,
      trackId: args.trackId || `qq:song:${songId(args)}`,
      url: coverUrl(cover, args.size || 300),
      headers: WEB_HEADERS,
    };
  }

  const TIERS = [
    {
      prefix: 'AI00',
      format: 'flac',
      mime: 'audio/flac',
      quality: 'master',
      size: ['size_new', 0],
    },
    { prefix: 'Q000', format: 'flac', mime: 'audio/flac', quality: 'atmos', size: ['size_new', 1] },
    {
      prefix: 'F000',
      format: 'flac',
      mime: 'audio/flac',
      quality: 'lossless',
      size: ['size_flac'],
    },
    { prefix: 'O801', format: 'ogg', mime: 'audio/ogg', quality: 640, size: ['size_new', 5] },
    { prefix: 'M800', format: 'mp3', mime: 'audio/mpeg', quality: 320, size: ['size_320mp3'] },
    { prefix: 'O800', format: 'ogg', mime: 'audio/ogg', quality: 320, size: ['size_new', 3] },
    { prefix: 'O600', format: 'ogg', mime: 'audio/ogg', quality: 192, size: ['size_192ogg'] },
    { prefix: 'C600', format: 'm4a', mime: 'audio/mp4', quality: 192, size: ['size_192aac'] },
    { prefix: 'M500', format: 'mp3', mime: 'audio/mpeg', quality: 128, size: ['size_128mp3'] },
    { prefix: 'O400', format: 'ogg', mime: 'audio/ogg', quality: 96, size: ['size_96ogg'] },
    { prefix: 'C400', format: 'm4a', mime: 'audio/mp4', quality: 96, size: ['size_96aac'] },
    { prefix: 'C200', format: 'm4a', mime: 'audio/mp4', quality: 48, size: ['size_48aac'] },
  ];
  const QUALITY_INDEX = {
    hires: 0,
    master: 0,
    atmos: 1,
    lossless: 2,
    flac: 2,
    high: 4,
    standard: 8,
  };

  async function getPlayback(args, context) {
    return resolvePlayback(args, context, false);
  }

  async function resolvePlayback(args, context, refreshed) {
    const song = await songInfo(songId(args));
    const quality = args.quality ?? 'high';
    let tiers =
      Number(quality) > 0
        ? TIERS.filter(
            (tier) => typeof tier.quality === 'number' && tier.quality <= Number(quality),
          )
        : TIERS.slice(QUALITY_INDEX[quality] ?? QUALITY_INDEX.high);
    if (args.preferFormat)
      tiers = [
        ...tiers.filter((t) => t.format === args.preferFormat),
        ...tiers.filter((t) => t.format !== args.preferFormat),
      ];
    const sizeOf = (tier) => tier.size.reduce((value, key) => value?.[key], song.file);
    if (tiers.some((tier) => sizeOf(tier) > 0)) tiers = tiers.filter((tier) => sizeOf(tier) > 0);
    const filename = (tier) =>
      `${tier.prefix}${song.file?.media_mid || song.mid + song.mid}.${tier.format}`;
    const credential = getCredential();
    const data = await musicRequest({
      module: 'music.vkey.GetVkey',
      method: 'UrlGetVkey',
      param: {
        uin: credential.str_musicid || '0',
        guid: getGuid(),
        filename: tiers.map(filename),
        songmid: tiers.map(() => song.mid),
        songtype: tiers.map(() => song.type || 0),
        ctx: 0,
      },
    });
    const expiresAt =
      data.expiration > 0 ? (context?.now ?? nowSecs()) + data.expiration : undefined;
    const cookie = credentialCookies(getCredential());
    const headers = { ...WEB_HEADERS, ...(cookie ? { Cookie: cookie } : {}) };
    const urls = [];
    for (const tier of tiers) {
      const info = data.midurlinfo.find((item) => item.filename === filename(tier));
      if (!info || info.result !== 0 || !info.purl) continue;
      urls.push({
        url: /^https?:\/\//.test(info.purl)
          ? info.purl
          : `https://isure.stream.qqmusic.qq.com/${info.purl.replace(/^\//, '')}`,
        headers,
        mimeType: tier.mime,
        quality: tier.quality,
        size: sizeOf(tier),
        expiresAt,
        bitrate: typeof tier.quality === 'number' ? tier.quality * 1000 : undefined,
      });
    }
    if (!urls.length) {
      const codes = data.midurlinfo.map((item) => item.result);
      const code = codes.find((value) => [1000, 104400, 104401].includes(value));
      if (code) throw new AuthExpired('QQ 音乐播放凭据已过期');
      if (codes.includes(104004))
        throw musicError('UPSTREAM_ERROR', 'QQ 音乐未签发播放授权', { upstreamCodes: codes });
      if (codes.includes(104013))
        throw musicError('UPSTREAM_ERROR', 'QQ 音乐限制了当前设备的播放权限', {
          upstreamCodes: codes,
        });
      const denied = codes.length > 0 && codes.every((value) => value === 104003);
      throw musicError(
        denied ? (credential.musickey ? 'VIP_REQUIRED' : 'AUTH_REQUIRED') : 'UPSTREAM_ERROR',
        'QQ 音乐未授权该歌曲的完整音频',
        { upstreamCodes: codes },
      );
    }
    return {
      trackId: `qq:song:${song.mid}`,
      provider: PROVIDER,
      duration: song.interval,
      urls,
      needRefreshBefore: expiresAt,
    };
  }

  async function refreshPlayback(args, context) {
    return getPlayback(
      { ...args, quality: args.quality ?? args.previous?.urls?.[0]?.quality },
      context,
    );
  }

  async function getLyrics(args) {
    const id = songId(args);
    const data = await musicRequest({
      module: 'music.musichallSong.PlayLyricInfo',
      method: 'GetPlayLyricInfo',
      preserveBool: true,
      param: {
        ...(/^\d+$/.test(id) ? { songId: Number(id) } : { songMid: id }),
        crypt: 1,
        lrc_t: 0,
        qrc: 1,
        qrc_t: 0,
        trans: 1,
        trans_t: 0,
        roma: 0,
        roma_t: 0,
        needSingingAnnotations: false,
        type: 1,
      },
    });
    const decode = (value) => (value ? (Number(data.crypt) === 1 ? qrcDecrypt(value) : value) : '');
    const lyric = decode(data.lyric);
    if (!lyric) throw musicError('NOT_FOUND', '无歌词');
    const krc = qrcToKrc(lyric);
    return {
      trackId: `qq:song:${id}`,
      provider: PROVIDER,
      format: krc ? 'krc' : 'lrc',
      text: krc || lyric,
      translation: decode(data.trans) || undefined,
    };
  }

  const SEARCH = {
    track: { code: 0, field: 'item_song', result: 'tracks', map: track },
    playlist: {
      code: 3,
      field: 'item_songlist',
      result: 'playlists',
      map: (item) => ({
        playlistId: `qq:playlist:${item.dissid}`,
        provider: PROVIDER,
        title: item.dissname,
        cover: coverUrl(item.logo),
        trackCount: item.song_count ?? item.songnum,
      }),
    },
    album: {
      code: 2,
      field: 'item_album',
      result: 'albums',
      map: (item) => ({
        albumId: `qq:album:${item.id ?? item.albumID}`,
        provider: PROVIDER,
        title: item.name || item.albumName,
        artist: (item.singer_list || []).map((s) => s.name).join(' / ') || item.singer,
        cover: coverUrl(item.pic) || photo(item.albummid),
        trackCount: item.song_count,
      }),
    },
    artist: {
      code: 1,
      field: 'singer',
      result: 'artists',
      map: (item) => ({
        artistId: `qq:singer:${item.singerMID}`,
        provider: PROVIDER,
        name: item.singerName,
        cover: coverUrl(item.singerPic) || photo(item.singerMID, 'T001'),
        trackCount: item.songNum,
        albumCount: item.albumNum,
      }),
    },
    programAlbum: {
      code: 15,
      field: 'item_audio',
      result: 'programAlbums',
      map: (item) => ({
        playlistId: `qq:program-album:${item.id}`,
        provider: PROVIDER,
        title: item.title || item.name,
        cover: coverUrl(item.pic),
      }),
    },
  };

  async function search(args) {
    const page = args.page || 1,
      pageSize = args.pageSize || 20;
    const type = SEARCH[args.type || 'track'];
    if (!type) return { page, pageSize, hasMore: false };
    const data = await musicRequest({
      module: 'music.search.SearchCgiService',
      method: 'DoSearchForQQMusicMobile',
      param: {
        searchid: getSearchId(),
        query: args.keyword,
        search_type: type.code,
        num_per_page: pageSize,
        page_num: page,
        highlight: false,
        grp: true,
      },
    });
    return {
      provider: PROVIDER,
      page,
      pageSize,
      total: data.meta.sum,
      hasMore: data.meta.nextpage > page,
      [type.result]: (data.body[type.field] || []).map(type.map).filter(Boolean),
    };
  }

  function playlistSummary(item) {
    return {
      playlistId: `qq:playlist:${item.tid}`,
      provider: PROVIDER,
      title: item.dirName,
      cover: coverUrl(item.picUrl),
      trackCount: item.songNum,
    };
  }

  async function getUserPlaylists(args = {}) {
    const cred = requireLogin();
    const data = await musicRequest({
      module: 'music.musicasset.PlaylistBaseRead',
      method: 'GetPlaylistByUin',
      param: { uin: cred.str_musicid || String(cred.musicid) },
    });
    const list = data.v_playlist.filter((item) => !item.invalid && item.tid).map(playlistSummary);
    const page = args.page || 1,
      size = args.pageSize || list.length;
    return {
      provider: PROVIDER,
      playlists: list.slice((page - 1) * size, page * size),
      hasMore: page * size < list.length,
    };
  }

  function collectionPage(kind, id, args, info, list, total, hasMore) {
    const page = args.page || 1,
      pageSize = args.pageSize || 50;
    const tracks = list.map(track).filter(Boolean);
    return {
      playlistId: `qq:${kind}:${id}`,
      provider: PROVIDER,
      ...info,
      page,
      pageSize,
      tracks,
      trackCount: total,
      hasMore: list.length > 0 && (hasMore ?? (page - 1) * pageSize + list.length < total),
    };
  }

  async function getPlaylist(args) {
    const { id, kind } = parseId(args.playlistId || args.providerId, 'playlist');
    const page = args.page || 1,
      num = args.pageSize || 50,
      offset = (page - 1) * num;
    if (kind === 'album' || kind === 'program-album') {
      const param = /^\d+$/.test(id) ? { albumId: Number(id) } : { albumMid: id };
      const [data, detail] = await musicBatch([
        {
          module: 'music.musichallAlbum.AlbumSongList',
          method: 'GetAlbumSongList',
          param: { ...param, begin: offset, num },
        },
        {
          module: 'music.musichallAlbum.AlbumInfoServer',
          method: 'GetAlbumDetail',
          param: /^\d+$/.test(id) ? param : { albumMId: id },
        },
      ]);
      const basic = detail.basicInfo;
      return collectionPage(
        kind,
        id,
        args,
        {
          title: basic.name,
          cover: coverUrl(basic.pic) || photo(basic.mid || basic.pmid || data.albumMid),
          creator: detail.singer?.singerList?.map((s) => s.name).join(' / '),
          description: basic.desc,
          year: basic.publishDate?.slice(0, 4),
          genre: basic.genre,
          language: basic.language,
          company: detail.company?.name,
        },
        data.songList.map((item) => item.songInfo),
        data.totalNum,
      );
    }
    if (kind === 'singer') {
      const [data, detail] = await musicBatch([
        {
          module: 'musichall.song_list_server',
          method: 'GetSingerSongList',
          param: { singerMid: id, order: 1, number: num, begin: offset },
        },
        {
          module: 'music.musichallSinger.SingerInfoInter',
          method: 'GetSingerDetail',
          param: {
            singer_mids: [id],
            group_singer: false,
            wiki_singer: false,
            ex_singer: true,
            pic: true,
            photos: false,
          },
        },
      ]);
      const singer = detail.singer_list[0];
      return collectionPage(
        kind,
        id,
        args,
        {
          title: singer.basic_info.name,
          cover: coverUrl(singer.pic.pic) || photo(id, 'T001'),
          description: singer.ex_info?.desc,
        },
        data.songList.map((item) => item.songInfo),
        data.totalNum,
      );
    }
    if (kind === 'top') {
      const data = await musicRequest({
        module: 'music.musicToplist.Toplist',
        method: 'GetDetail',
        preserveBool: true,
        param: { topId: Number(id), offset, num, withTags: true },
      });
      return collectionPage(
        kind,
        id,
        args,
        { title: data.data.title, cover: coverUrl(data.data.frontPicUrl || data.data.headPicUrl) },
        data.songInfoList,
        data.data.totalNum,
      );
    }
    if (kind !== 'playlist') throw musicError('NOT_FOUND', `未知集合类型: ${kind}`);
    const data = await musicRequest({
      module: 'music.srfDissInfo.DissInfo',
      method: 'CgiGetDiss',
      param: {
        disstid: Number(id),
        dirid: 0,
        tag: true,
        song_begin: offset,
        song_num: num,
        userinfo: true,
        orderlist: true,
        onlysonglist: false,
      },
    });
    return collectionPage(
      kind,
      id,
      args,
      {
        title: data.dirinfo.title,
        cover: coverUrl(data.dirinfo.picurl),
        description: data.dirinfo.desc,
      },
      data.songlist,
      data.total_song_num,
      Boolean(data.hasmore),
    );
  }

  function decodeEntities(text) {
    return String(text || '')
      .replace(/&#(\d+);/g, (_, n) => String.fromCodePoint(Number(n)))
      .replace(/&amp;/g, '&')
      .replace(/&lt;/g, '<')
      .replace(/&gt;/g, '>')
      .replace(/&quot;/g, '"');
  }

  let categoriesFlight;
  async function categories() {
    if (!categoriesFlight) {
      categoriesFlight = jsonRequest(
        'https://c.y.qq.com/splcloud/fcgi-bin/fcg_get_diss_tag_conf.fcg?format=json&g_tk=5381&inCharset=utf8&outCharset=utf-8',
        { headers: WEB_HEADERS },
      )
        .then((body) => {
          checkCode(body.code, 'tag_conf');
          return body.data.categories.map((group) => ({
            name: decodeEntities(group.categoryGroupName),
            categories: group.items.map((item) => ({
              name: decodeEntities(item.categoryName),
              id: item.categoryId,
            })),
          }));
        })
        .catch((error) => {
          categoriesFlight = undefined;
          throw error;
        });
    }
    return categoriesFlight;
  }

  async function getDiscoverCategories() {
    return { provider: PROVIDER, groups: await categories() };
  }

  async function getRecommendedPlaylists(args = {}) {
    const page = args.page || 1,
      size = args.pageSize || 24;
    if (args.category) {
      const category = (await categories())
        .flatMap((group) => group.categories)
        .find((item) => item.name === args.category);
      if (!category) return { provider: PROVIDER, playlists: [], hasMore: false };
      const offset = (page - 1) * size;
      const query = new URLSearchParams({
        format: 'json',
        g_tk: '5381',
        inCharset: 'utf8',
        outCharset: 'utf-8',
        platform: 'yqq.json',
        needNewCode: '0',
        categoryId: String(category.id),
        sortId: '5',
        sin: String(offset),
        ein: String(offset + size - 1),
      });
      const body = await jsonRequest(
        `https://c.y.qq.com/splcloud/fcgi-bin/fcg_get_diss_by_tag.fcg?${query}`,
        { headers: WEB_HEADERS },
      );
      checkCode(body.code, 'diss_by_tag');
      return {
        provider: PROVIDER,
        hasMore: offset + body.data.list.length < body.data.sum,
        playlists: body.data.list.map((item) => ({
          playlistId: `qq:playlist:${item.dissid}`,
          provider: PROVIDER,
          title: decodeEntities(item.dissname),
          cover: coverUrl(item.imgurl),
          playCount: item.listennum,
          description: decodeEntities(item.introduction),
        })),
      };
    }
    const data = await musicRequest({
      module: 'music.playlist.PlaylistSquare',
      method: 'GetRecommendFeed',
      param: { From: (page - 1) * size, Size: size },
    });
    return {
      provider: PROVIDER,
      hasMore: Boolean(data.HasMore),
      playlists: data.List.map(({ Playlist: { basic } }) => ({
        playlistId: `qq:playlist:${basic.tid}`,
        provider: PROVIDER,
        title: basic.title,
        cover: coverUrl(basic.cover.default_url),
        trackCount: basic.song_cnt,
        playCount: basic.play_cnt,
        description: basic.desc,
      })),
    };
  }

  function feedRequest(cursor) {
    return musicRequest({
      module: 'music.recommend.RecommendFeed',
      method: 'get_recommend_feed',
      param: {
        direction: cursor.page === 1 ? 0 : 1,
        page: cursor.page,
        s_num: cursor.sNum,
        v_cache: cursor.vCache,
      },
    });
  }

  function feedCards(feed) {
    return (feed.v_shelf || []).flatMap((shelf) =>
      (shelf.v_niche || []).flatMap((niche) => niche.v_card || []),
    );
  }

  function feedTitle(value) {
    const content = String(value.title_content || '').trim();
    return String(value.title_template || content)
      .replace(/\{String\}/g, content)
      .trim();
  }

  function songCard(card) {
    return (
      [200, 1700].includes(Number(card.type)) || [10046, 20007].includes(Number(card.jumptype))
    );
  }

  function expandFeedCard(card) {
    if (!songCard(card)) return [card];
    // SongList is a multi-song card; Tracks contains full metadata for only some of its songs.
    for (const song of card.card_extra_info?.Tracks || []) track(song);
    const list = card.card_extra_info?.SongList;
    return list?.length
      ? list.map((song) => ({
          ...card,
          id: song.ID,
          title: song.Name,
          subtitle: song.SingerName,
          cover: song.Cover,
          songMid: song.MID,
        }))
      : [card];
  }

  async function getDailyRecommend(args, context) {
    requireLogin();
    const feed = await feedRequest({ page: 1, sNum: 0, vCache: [] });
    const card = feedCards(feed).find((item) => item.type === 500 && item.subtype === 510);
    if (!card) throw musicError('NOT_FOUND', 'QQ 音乐未返回每日推荐歌单');
    const playlist = await getPlaylist(
      { playlistId: `qq:playlist:${card.id}`, pageSize: 60 },
      context,
    );
    return { provider: PROVIDER, tracks: playlist.tracks };
  }

  function cardImage(card) {
    const image = card.cover;
    return coverUrl(typeof image === 'string' ? image : image?.default_url || image?.medium_url);
  }

  function cardItem(card) {
    card = {
      ...card,
      type: Number(card.type),
      subtype: Number(card.subtype),
      jumptype: Number(card.jumptype),
    };
    const id = String(card.id || '').trim();
    if (!id || id === '0') return null;
    const base = {
      provider: PROVIDER,
      title: card.title || card.title_content || card.subtitle || '',
      cover: cardImage(card),
    };
    if (songCard(card)) {
      return {
        type: 'song',
        item: songs.has(id)
          ? track(songs.get(id))
          : {
              ...base,
              trackId: `qq:song:${card.songMid || id}`,
              artists: card.subtitle ? [card.subtitle] : [],
            },
      };
    }
    if (card.type === 500 && (card.subtype === 510 || card.subtype === 513)) {
      return { type: 'entry', item: { ...base, kind: 'playlist', target: `qq:playlist:${id}` } };
    }
    if (card.jumptype === 10014 && card.type === 500) {
      return { type: 'playlist', item: { ...base, playlistId: `qq:playlist:${id}` } };
    }
    if (card.jumptype === 10025 && card.type === 400) {
      return { type: 'program', item: { ...base, playlistId: `qq:program-album:${id}` } };
    }
    if (card.jumptype === 10002 || (card.type === 400 && [414, 416].includes(card.subtype))) {
      return {
        type: 'album',
        item: {
          ...base,
          albumId: `qq:album:${id}`,
          title: card.subtype === 414 ? card.subtitle || base.title : base.title,
          artist: card.subtype === 414 ? base.title : card.subtitle,
        },
      };
    }
    if (card.jumptype === 10005 || card.type === 1000) {
      return { type: 'toplist', item: { ...base, toplistId: `qq:top:${id}` } };
    }
    if (
      card.jumptype === 20001 ||
      (card.type === 900 && card.subtype === 991 && /雷达/.test(base.title))
    ) {
      return {
        type: 'entry',
        item: {
          ...base,
          kind: 'radio',
          target: { radioChannelId: card.jumptype === 20001 ? 'qq:radio:guess' : 'qq:radio:radar' },
        },
      };
    }
    return null;
  }

  function homeError(error, module) {
    let detail;
    try {
      detail = JSON.parse(error.message);
    } catch {
      detail = { code: 'UPSTREAM_ERROR', message: String(error) };
    }
    return { code: detail.code, message: `${module}：${detail.message}` };
  }

  async function newAlbumShelf(start = 0) {
    const data = await musicRequest({
      module: 'newalbum.NewAlbumServer',
      method: 'get_new_album_info',
      param: { area: 1, num: 20, start },
    });
    if (!Array.isArray(data.albums)) throw musicError('PARSE_ERROR', '新碟响应缺少 albums');
    const albums = data.albums,
      nextOffset = start + albums.length;
    const total = data.total == null ? undefined : Number(data.total);
    const shelf = {
      id: 'qq:home:new-albums',
      title: '新碟上架',
      type: 'album',
      items: albums
        .filter((album) => album.id || album.mid)
        .map((album) => ({
          albumId: `qq:album:${album.id || album.mid}`,
          provider: PROVIDER,
          title: album.name || album.title,
          artist: (album.singers || []).map((singer) => singer.name).join(' / '),
          cover: coverUrl(album.pic) || photo(album.pmid || album.mid),
          releaseDate: album.release_time,
        })),
    };
    return {
      shelf,
      nextOffset,
      total,
      hasMore:
        albums.length > 0 && (Number.isFinite(total) ? nextOffset < total : albums.length === 20),
    };
  }

  function albumCursor(result, page) {
    return { phase: 'albums', albumOffset: result.nextOffset, albumTotal: result.total, page };
  }

  async function getHomeFeed(args = {}) {
    if (args.cursor?.phase === 'albums') {
      const albums = await newAlbumShelf(Math.max(0, Number(args.cursor.albumOffset) || 0));
      return {
        provider: PROVIDER,
        shelves: albums.shelf.items.length ? [albums.shelf] : [],
        hasMore: albums.hasMore,
        cursor: albums.hasMore ? albumCursor(albums, (args.cursor.page || 1) + 1) : undefined,
      };
    }
    if (!getCredential().musickey) {
      const page = args.cursor?.page || 1;
      const result = await getRecommendedPlaylists({ page });
      return {
        provider: PROVIDER,
        shelves: [
          { id: 'qq:home:playlists', title: '推荐歌单', type: 'playlist', items: result.playlists },
        ],
        hasMore: result.hasMore,
        cursor: result.hasMore ? { page: page + 1 } : undefined,
      };
    }
    const cursor = { page: 1, sNum: 0, vCache: [], titles: {}, recentPages: [], ...args.cursor };
    let nextCursor = cursor;
    cursor.titles = { ...cursor.titles };
    const raw = [];
    const sourceErrors = [];
    let hasMore = true;
    for (let index = 0; index < Math.max(1, Math.min(6, Number(args.pageCount) || 1)); index++) {
      const feed = await feedRequest(cursor);
      checkCode(feed.retcode ?? 0, 'get_recommend_feed');
      if (!Array.isArray(feed.v_shelf)) throw musicError('PARSE_ERROR', '首页响应缺少 v_shelf');
      if (!feed.v_shelf.length) {
        hasMore = false;
        break;
      }
      const fingerprint = hash33(
        JSON.stringify(
          feed.v_shelf.map((shelf) => [
            shelf.id,
            feedTitle(shelf),
            feedCards({ v_shelf: [shelf] }).map((card) => [
              card.type,
              card.subtype,
              card.id,
              card.title,
              (card.card_extra_info?.SongList || []).map((song) => song.ID),
            ]),
          ]),
        ),
      );
      if (cursor.recentPages.includes(fingerprint)) {
        hasMore = false;
        break;
      }
      cursor.recentPages = [...cursor.recentPages, fingerprint].slice(-4);
      raw.push(...feed.v_shelf);
      cursor.page++;
      cursor.sNum += feed.v_shelf.length;
      cursor.vCache = [
        ...new Set([...cursor.vCache, ...feed.v_shelf.map((shelf) => String(shelf.id))]),
      ];
      if (cursor.page > 30) {
        hasMore = false;
        break;
      }
    }
    const cards = feedCards({ v_shelf: raw }).flatMap(expandFeedCard);
    const ids = [...new Set(cards.filter(songCard).map((card) => String(card.id || '')))].filter(
      (id) => id && id !== '0' && !songs.has(id),
    );
    // Feed cards use numeric song IDs. subid is not a song MID.
    await Promise.all(
      Array.from({ length: Math.ceil(ids.length / 100) }, (_, i) =>
        queryTracks(ids.slice(i * 100, i * 100 + 100)).catch((error) =>
          sourceErrors.push(homeError(error, '歌曲详情')),
        ),
      ),
    );
    const shelves = [];
    const byId = new Map();
    for (const shelf of raw) {
      const title = feedTitle(shelf) || cursor.titles[shelf.id] || '';
      cursor.titles[shelf.id] = title;
      let first = true;
      for (const niche of shelf.v_niche || [])
        for (const card of (niche.v_card || []).flatMap(expandFeedCard)) {
          const mapped = cardItem(card);
          if (!mapped?.item) continue;
          const nicheTitle = feedTitle(niche);
          // The desktop uses one paged song panel per group, not one tall panel per alternating card.
          const id = `qq:shelf:${shelf.id}:${hash33(title)}:${hash33(nicheTitle)}:${mapped.type}`;
          let run = byId.get(id);
          if (!run) {
            run = { id, title: nicheTitle || (first ? title : ''), type: mapped.type, items: [] };
            shelves.push(run);
            byId.set(id, run);
          }
          first = false;
          const item = mapped.item;
          const key = (value) =>
            value.trackId ||
            value.playlistId ||
            value.albumId ||
            value.toplistId ||
            JSON.stringify(value.target);
          if (!run.items.some((existing) => key(existing) === key(item))) run.items.push(item);
        }
    }
    if (!args.cursor) {
      const guessEntries = shelves
        .filter((shelf) => shelf.type === 'entry')
        .flatMap((shelf) => shelf.items)
        .filter((item) => item.target?.radioChannelId === 'qq:radio:guess' && !item.cover);
      const [albums, radio, guess] = await Promise.all([
        shelves.some((shelf) => shelf.type === 'album')
          ? null
          : newAlbumShelf().catch((error) => {
              sourceErrors.push(homeError(error, '新碟上架'));
              return null;
            }),
        args.keepRadio
          ? null
          : getRadioTracks({ channelId: 'qq:radio:radar', seedTrackIds: args.seedTrackIds }).catch(
              (error) => {
                sourceErrors.push(homeError(error, '私人雷达'));
                return null;
              },
            ),
        guessEntries.length
          ? getRadioTracks({ channelId: 'qq:radio:guess', count: 5 }).catch((error) => {
              sourceErrors.push(homeError(error, '猜你喜欢'));
              return null;
            })
          : null,
      ]);
      if (guess?.tracks.length)
        for (const item of guessEntries) {
          item.cover = guess.tracks.find((song) => song.cover)?.cover;
          item.previewTracks = guess.tracks;
        }
      if (albums) {
        if (albums.shelf.items.length) shelves.push(albums.shelf);
        // Once the footer follows new albums, continue that collection rather than the unrelated song feed.
        hasMore = albums.hasMore;
        nextCursor = albumCursor(albums, cursor.page);
      }
      if (radio?.tracks.length) {
        shelves.unshift({
          id: 'qq:home:radar:radio',
          title: '私人雷达',
          type: 'radio',
          items: [
            {
              provider: PROVIDER,
              title: '私人雷达',
              kind: 'radio',
              cover: radio.tracks[0].cover,
              target: { radioChannelId: 'qq:radio:radar' },
            },
          ],
          songs: radio.tracks,
        });
        for (const shelf of shelves.filter((shelf) => shelf.type === 'entry')) {
          shelf.items = shelf.items.filter(
            (item) => item.target?.radioChannelId !== 'qq:radio:radar',
          );
        }
      }
    }
    return {
      provider: PROVIDER,
      shelves: shelves.filter((shelf) => shelf.items.length),
      hasMore,
      cursor: hasMore ? nextCursor : undefined,
      ...(sourceErrors.length ? { sourceErrors } : {}),
    };
  }

  async function getRadioChannels() {
    return {
      provider: PROVIDER,
      channels: [
        { channelId: 'qq:radio:guess', title: '猜你喜欢' },
        { channelId: 'qq:radio:radar', title: '私人雷达' },
      ],
      hasMore: false,
    };
  }

  async function getRadioTracks(args = {}) {
    requireLogin();
    const radar = args.channelId === 'qq:radio:radar';
    if (args.channelId && !['qq:radio:guess', 'qq:radio:radar'].includes(args.channelId)) {
      throw musicError('NOT_FOUND', '未知电台');
    }
    const mids = [...new Set((args.seedTrackIds || []).map((id) => parseId(id).id))].slice(
      0,
      radar ? 10 : 1,
    );
    const missing = mids.filter((id) => !songs.has(id));
    if (missing.length) await queryTracks(missing);
    const seeds = mids.map((id) => songs.get(id)?.id).filter(Boolean);
    const data = await musicRequest(
      radar
        ? {
            module: 'music.recommend.TrackRelationServer',
            method: 'GetRadarSong',
            param: { Page: args.page || 1, ReqType: 0, FavSongs: seeds, EntranceSongs: [] },
          }
        : {
            module: 'music.radioProxy.MbTrackRadioSvr',
            method: 'get_radio_track',
            param: { id: 99, num: args.count || 10, from: 0, scene: 0, song_ids: seeds },
          },
    );
    const list = radar ? data.VecSongs.map((item) => item.Track) : data.tracks;
    return {
      provider: PROVIDER,
      tracks: list
        .filter((song) => !seeds.includes(song.id))
        .map(track)
        .filter(Boolean),
      hasMore: radar ? Boolean(data.HasMore) : list.length > 0,
    };
  }

  // QQMusicApi sign and QIMEI algorithms; pure JS AES/RSA for CJRT.
  function getSearchId() {
    const e = randInt(1, 20);
    const t = e * 18014398509481984;
    const n = randInt(0, 4194304) * 4294967296;
    const r = Math.round(Date.now()) % (24 * 60 * 60 * 1000);
    return String(t + n + r);
  }

  function getGuid() {
    return randHex(32);
  }

  function boolToInt(data) {
    if (typeof data === 'boolean') return data ? 1 : 0;
    if (Array.isArray(data)) return data.map(boolToInt);
    if (data && typeof data === 'object') {
      const out = {};
      for (const k of Object.keys(data)) out[k] = boolToInt(data[k]);
      return out;
    }
    return data;
  }

  function calcMd5(...parts) {
    const h = createHash('md5');
    for (const p of parts) {
      h.update(typeof p === 'string' ? utf8(p) : new Uint8Array(p));
    }
    return h.digest('hex');
  }

  function sha1HexUpper(bytes) {
    return createHash('sha1').update(Buffer.from(bytes)).digest('hex').toUpperCase();
  }

  const ZZC_PART1_IDX = [23, 14, 6, 36, 16, 7, 19];
  const ZZC_PART2_IDX = [16, 1, 32, 12, 19, 27, 8, 5];
  const ZZC_SCRAMBLE = [
    89, 39, 179, 150, 218, 82, 58, 252, 177, 52, 186, 123, 120, 64, 242, 133, 143, 161, 121, 179,
  ];

  function zzcSign(payloadStr) {
    const hex = sha1HexUpper(utf8(payloadStr));
    const part1 = ZZC_PART1_IDX.map((i) => hex[i]).join('');
    const part2 = ZZC_PART2_IDX.map((i) => hex[i]).join('');
    const part3 = new Uint8Array(20);
    for (let i = 0; i < ZZC_SCRAMBLE.length; i++) {
      part3[i] = ZZC_SCRAMBLE[i] ^ parseInt(hex.slice(i * 2, i * 2 + 2), 16);
    }
    const b64 = Buffer.from(part3)
      .toString('base64')
      .replace(/[\\/+=]/g, '');
    return `zzc${part1}${b64}${part2}`.toLowerCase();
  }
  function aesCbcEncrypt(key, plain, iv = key) {
    const cipher = createCipheriv('aes-128-cbc', key, iv);
    return new Uint8Array([...cipher.update(plain), ...cipher.final()]);
  }
  function rsaEncryptPkcs1(bytes) {
    return publicEncrypt(
      {
        key: '-----BEGIN RSA PUBLIC KEY-----\nMIGJAoGBAMQjGDCi61/CgnFwZB552A/sUb2poi5LSrN9HyBaSuRNkozaJYefZqNC\nkFFmMxKhJ/r4oka9qvY5GEF+kNfJW1kIqmotD4UuSmdwKUpUisHC/o8fJS+4JvSs\nhquaAOfOR9ACpW58S1HriJrMYMpq28n3LoH00xsd10ZIBSZFMKsdAgMBAAE=\n-----END RSA PUBLIC KEY-----',
        padding: 1,
      },
      bytes,
    );
  }

  function bytesToBigInt(u8) {
    let v = 0n;
    for (const b of u8) v = (v << 8n) | BigInt(b);
    return v;
  }

  function bigIntToBytes(v, len) {
    const out = new Uint8Array(len);
    for (let i = len - 1; i >= 0; i--) {
      out[i] = Number(v & 0xffn);
      v >>= 8n;
    }
    return out;
  }

  function deviceToken(value) {
    return toB64(aesCbcEncrypt(utf8('lvcwmSYVr2Axv1gn'), utf8(value), utf8('Zs0ntDqG2jyhKN0c')));
  }

  function generateImei() {
    const digits = [];
    for (let i = 0; i < 14; i++) digits.push(randInt(0, 9));
    let sum = 0;
    digits.forEach((d, idx) => {
      let x = d;
      if (idx % 2 === 1) {
        x *= 2;
        if (x > 9) x -= 9;
      }
      sum += x;
    });
    digits.push((10 - (sum % 10)) % 10);
    return digits.join('');
  }

  function getDevice() {
    let d = getJson('device');
    if (d && d.androidId) {
      if (!d.openUdid2) {
        d.openUdid2 = randHex(32);
        setJson('device', d);
      }
      return d;
    }
    d = {
      display: `QMAPI.${randInt(100000, 999999)}.001`,
      product: 'iarim',
      device: 'sagit',
      board: 'eomam',
      brand: 'Xiaomi',
      model: 'MI 6',
      fingerprint: `xiaomi/iarim/sagit:10/eomam.200122.001/${randInt(1000000, 9999999)}:user/release-keys`,
      bootId: randHex(32),
      procVersion: `Linux 5.4.0-54-generic-${randChoice(
        'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789',
        8,
      )}`,
      imei: generateImei(),
      androidId: randHex(16),
      openUdid: randHex(32),
      openUdid2: randHex(32),
      release: '10',
      sdk: 29,
    };
    setJson('device', d);
    return d;
  }

  function randomBeaconId() {
    const now = new Date();
    const timeMonth = `${now.getUTCFullYear()}-${String(now.getUTCMonth() + 1).padStart(2, '0')}-01`;
    const rand1 = randInt(100000, 999999);
    const rand2 = randInt(100000000, 999999999);
    const kMulti = new Set([1, 2, 13, 14, 17, 18, 21, 22, 25, 26, 29, 30, 33, 34, 37, 38]);
    let s = '';
    for (let i = 1; i <= 40; i++) {
      if (kMulti.has(i)) s += `k${i}:${timeMonth}${rand1}.${rand2}`;
      else if (i === 3) s += 'k3:0000000000000000';
      else if (i === 4) s += `k4:${randChoice(HEX_CHARS.slice(1), 16)}`;
      else s += `k${i}:${randInt(0, 9999)}`;
      s += ';';
    }
    return s;
  }

  function pad2(n) {
    return String(n).padStart(2, '0');
  }
  function fmtDateTimeUtc(d) {
    return `${d.getUTCFullYear()}-${pad2(d.getUTCMonth() + 1)}-${pad2(d.getUTCDate())} ${pad2(
      d.getUTCHours(),
    )}:${pad2(d.getUTCMinutes())}:${pad2(d.getUTCSeconds())}`;
  }

  function randomQimeiPayload(device) {
    const fixedRand = randInt(0, 14400);
    const uptimes = fmtDateTimeUtc(new Date(Date.now() - fixedRand * 1000));
    const reserved = {
      harmony: '0',
      clone: '0',
      containe: '',
      oz: deviceToken(device.androidId),
      oo: deviceToken(device.model),
      kelong: '0',
      uptimes,
      multiUser: '0',
      bod: device.board,
      brd: device.brand,
      ip: `192.168.${parseInt(calcMd5(device.androidId).slice(0, 2), 16)}.${(parseInt(calcMd5(device.androidId).slice(2, 4), 16) % 253) + 2}`,
      dv: device.device,
      firstLevel: String(device.sdk),
      manufact: device.brand,
      name: device.product,
      pre: '0',
      av: QIMEI_APP_VERSION,
      ch: '',
      host: 'se.infra',
      kernel: device.procVersion,
    };
    return {
      androidId: device.androidId,
      platformId: 1,
      appKey: QIMEI_APP_KEY,
      appVersion: QIMEI_APP_VERSION,
      beaconIdSrc: randomBeaconId(),
      brand: device.brand,
      channelId: CHANNEL_ID,
      cid: '',
      imei: device.imei,
      imsi: '',
      mac: '',
      model: device.model,
      networkType: 'wifi',
      oaid: '',
      osVersion: `Android ${device.release},level ${device.sdk}`,
      qimei: '',
      qimei36: '',
      sdkVersion: QIMEI_SDK_VERSION,
      targetSdkVersion: '30',
      audit: '',
      userId: '{}',
      packageId: PACKAGE_ID,
      deviceType: 'Phone',
      sdkName: '',
      reserved: JSON.stringify(reserved),
    };
  }

  // QQMusicApi / QQMusicDecoder custom 3DES (GPL-3.0); QRC is not standard DES.
  const QRC_SBOX = [
    [
      14, 4, 13, 1, 2, 15, 11, 8, 3, 10, 6, 12, 5, 9, 0, 7, 0, 15, 7, 4, 14, 2, 13, 1, 10, 6, 12,
      11, 9, 5, 3, 8, 4, 1, 14, 8, 13, 6, 2, 11, 15, 12, 9, 7, 3, 10, 5, 0, 15, 12, 8, 2, 4, 9, 1,
      7, 5, 11, 3, 14, 10, 0, 6, 13,
    ],
    [
      15, 1, 8, 14, 6, 11, 3, 4, 9, 7, 2, 13, 12, 0, 5, 10, 3, 13, 4, 7, 15, 2, 8, 15, 12, 0, 1, 10,
      6, 9, 11, 5, 0, 14, 7, 11, 10, 4, 13, 1, 5, 8, 12, 6, 9, 3, 2, 15, 13, 8, 10, 1, 3, 15, 4, 2,
      11, 6, 7, 12, 0, 5, 14, 9,
    ],
    [
      10, 0, 9, 14, 6, 3, 15, 5, 1, 13, 12, 7, 11, 4, 2, 8, 13, 7, 0, 9, 3, 4, 6, 10, 2, 8, 5, 14,
      12, 11, 15, 1, 13, 6, 4, 9, 8, 15, 3, 0, 11, 1, 2, 12, 5, 10, 14, 7, 1, 10, 13, 0, 6, 9, 8, 7,
      4, 15, 14, 3, 11, 5, 2, 12,
    ],
    [
      7, 13, 14, 3, 0, 6, 9, 10, 1, 2, 8, 5, 11, 12, 4, 15, 13, 8, 11, 5, 6, 15, 0, 3, 4, 7, 2, 12,
      1, 10, 14, 9, 10, 6, 9, 0, 12, 11, 7, 13, 15, 1, 3, 14, 5, 2, 8, 4, 3, 15, 0, 6, 10, 10, 13,
      8, 9, 4, 5, 11, 12, 7, 2, 14,
    ],
    [
      2, 12, 4, 1, 7, 10, 11, 6, 8, 5, 3, 15, 13, 0, 14, 9, 14, 11, 2, 12, 4, 7, 13, 1, 5, 0, 15,
      10, 3, 9, 8, 6, 4, 2, 1, 11, 10, 13, 7, 8, 15, 9, 12, 5, 6, 3, 0, 14, 11, 8, 12, 7, 1, 14, 2,
      13, 6, 15, 0, 9, 10, 4, 5, 3,
    ],
    [
      12, 1, 10, 15, 9, 2, 6, 8, 0, 13, 3, 4, 14, 7, 5, 11, 10, 15, 4, 2, 7, 12, 9, 5, 6, 1, 13, 14,
      0, 11, 3, 8, 9, 14, 15, 5, 2, 8, 12, 3, 7, 0, 4, 10, 1, 13, 11, 6, 4, 3, 2, 12, 9, 5, 15, 10,
      11, 14, 1, 7, 6, 0, 8, 13,
    ],
    [
      4, 11, 2, 14, 15, 0, 8, 13, 3, 12, 9, 7, 5, 10, 6, 1, 13, 0, 11, 7, 4, 9, 1, 10, 14, 3, 5, 12,
      2, 15, 8, 6, 1, 4, 11, 13, 12, 3, 7, 14, 10, 15, 6, 8, 0, 5, 9, 2, 6, 11, 13, 8, 1, 4, 10, 7,
      9, 5, 0, 15, 14, 2, 3, 12,
    ],
    [
      13, 2, 8, 4, 6, 15, 11, 1, 10, 9, 3, 14, 5, 0, 12, 7, 1, 15, 13, 8, 10, 3, 7, 4, 12, 5, 6, 11,
      0, 14, 9, 2, 7, 11, 4, 1, 9, 12, 14, 2, 0, 6, 10, 13, 15, 3, 5, 8, 2, 1, 14, 7, 4, 10, 8, 13,
      15, 12, 9, 0, 3, 5, 6, 11,
    ],
  ];
  const QRC_PERM_C = [
    56, 48, 40, 32, 24, 16, 8, 0, 57, 49, 41, 33, 25, 17, 9, 1, 58, 50, 42, 34, 26, 18, 10, 2, 59,
    51, 43, 35,
  ];
  const QRC_PERM_D = [
    62, 54, 46, 38, 30, 22, 14, 6, 61, 53, 45, 37, 29, 21, 13, 5, 60, 52, 44, 36, 28, 20, 12, 4, 27,
    19, 11, 3,
  ];
  const QRC_COMPRESSION = [
    13, 16, 10, 23, 0, 4, 2, 27, 14, 5, 20, 9, 22, 18, 11, 3, 25, 7, 15, 6, 26, 19, 12, 1, 40, 51,
    30, 36, 46, 54, 29, 39, 50, 44, 32, 47, 43, 48, 38, 55, 33, 52, 45, 41, 49, 35, 28, 31,
  ];
  const QRC_RND_SHIFT = [1, 1, 2, 2, 2, 2, 2, 2, 1, 2, 2, 2, 2, 2, 2, 1];
  const QRC_KEY = new Uint8Array(Buffer.from('!@#)(*$%123ZXC!@!@#)(NHL', 'latin1'));
  const QRC_ENCRYPT = 1;
  const QRC_DECRYPT = 0;

  const qrcSboxBit = (a) => (a & 32) | ((a & 31) >>> 1) | ((a & 1) << 4);

  function qrcInitialPermutation(d) {
    const v0 = (d[0] | (d[1] << 8) | (d[2] << 16) | (d[3] << 24)) >>> 0;
    const v1 = (d[4] | (d[5] << 8) | (d[6] << 16) | (d[7] << 24)) >>> 0;
    const s0 =
      ((((v1 >>> 6) & 1) << 31) |
        (((v1 >>> 14) & 1) << 30) |
        (((v1 >>> 22) & 1) << 29) |
        (((v1 >>> 30) & 1) << 28) |
        (((v0 >>> 6) & 1) << 27) |
        (((v0 >>> 14) & 1) << 26) |
        (((v0 >>> 22) & 1) << 25) |
        (((v0 >>> 30) & 1) << 24) |
        (((v1 >>> 4) & 1) << 23) |
        (((v1 >>> 12) & 1) << 22) |
        (((v1 >>> 20) & 1) << 21) |
        (((v1 >>> 28) & 1) << 20) |
        (((v0 >>> 4) & 1) << 19) |
        (((v0 >>> 12) & 1) << 18) |
        (((v0 >>> 20) & 1) << 17) |
        (((v0 >>> 28) & 1) << 16) |
        (((v1 >>> 2) & 1) << 15) |
        (((v1 >>> 10) & 1) << 14) |
        (((v1 >>> 18) & 1) << 13) |
        (((v1 >>> 26) & 1) << 12) |
        (((v0 >>> 2) & 1) << 11) |
        (((v0 >>> 10) & 1) << 10) |
        (((v0 >>> 18) & 1) << 9) |
        (((v0 >>> 26) & 1) << 8) |
        (((v1 >>> 0) & 1) << 7) |
        (((v1 >>> 8) & 1) << 6) |
        (((v1 >>> 16) & 1) << 5) |
        (((v1 >>> 24) & 1) << 4) |
        (((v0 >>> 0) & 1) << 3) |
        (((v0 >>> 8) & 1) << 2) |
        (((v0 >>> 16) & 1) << 1) |
        ((v0 >>> 24) & 1)) >>>
      0;
    const s1 =
      ((((v1 >>> 7) & 1) << 31) |
        (((v1 >>> 15) & 1) << 30) |
        (((v1 >>> 23) & 1) << 29) |
        (((v1 >>> 31) & 1) << 28) |
        (((v0 >>> 7) & 1) << 27) |
        (((v0 >>> 15) & 1) << 26) |
        (((v0 >>> 23) & 1) << 25) |
        (((v0 >>> 31) & 1) << 24) |
        (((v1 >>> 5) & 1) << 23) |
        (((v1 >>> 13) & 1) << 22) |
        (((v1 >>> 21) & 1) << 21) |
        (((v1 >>> 29) & 1) << 20) |
        (((v0 >>> 5) & 1) << 19) |
        (((v0 >>> 13) & 1) << 18) |
        (((v0 >>> 21) & 1) << 17) |
        (((v0 >>> 29) & 1) << 16) |
        (((v1 >>> 3) & 1) << 15) |
        (((v1 >>> 11) & 1) << 14) |
        (((v1 >>> 19) & 1) << 13) |
        (((v1 >>> 27) & 1) << 12) |
        (((v0 >>> 3) & 1) << 11) |
        (((v0 >>> 11) & 1) << 10) |
        (((v0 >>> 19) & 1) << 9) |
        (((v0 >>> 27) & 1) << 8) |
        (((v1 >>> 1) & 1) << 7) |
        (((v1 >>> 9) & 1) << 6) |
        (((v1 >>> 17) & 1) << 5) |
        (((v1 >>> 25) & 1) << 4) |
        (((v0 >>> 1) & 1) << 3) |
        (((v0 >>> 9) & 1) << 2) |
        (((v0 >>> 17) & 1) << 1) |
        ((v0 >>> 25) & 1)) >>>
      0;
    return [s0, s1];
  }

  function qrcInversePermutation(s0, s1) {
    const out = new Uint8Array(8);
    const b = (x, n) => (x >>> n) & 1;
    out[3] =
      (b(s1, 24) << 7) |
      (b(s0, 24) << 6) |
      (b(s1, 16) << 5) |
      (b(s0, 16) << 4) |
      (b(s1, 8) << 3) |
      (b(s0, 8) << 2) |
      (b(s1, 0) << 1) |
      b(s0, 0);
    out[2] =
      (b(s1, 25) << 7) |
      (b(s0, 25) << 6) |
      (b(s1, 17) << 5) |
      (b(s0, 17) << 4) |
      (b(s1, 9) << 3) |
      (b(s0, 9) << 2) |
      (b(s1, 1) << 1) |
      b(s0, 1);
    out[1] =
      (b(s1, 26) << 7) |
      (b(s0, 26) << 6) |
      (b(s1, 18) << 5) |
      (b(s0, 18) << 4) |
      (b(s1, 10) << 3) |
      (b(s0, 10) << 2) |
      (b(s1, 2) << 1) |
      b(s0, 2);
    out[0] =
      (b(s1, 27) << 7) |
      (b(s0, 27) << 6) |
      (b(s1, 19) << 5) |
      (b(s0, 19) << 4) |
      (b(s1, 11) << 3) |
      (b(s0, 11) << 2) |
      (b(s1, 3) << 1) |
      b(s0, 3);
    out[7] =
      (b(s1, 28) << 7) |
      (b(s0, 28) << 6) |
      (b(s1, 20) << 5) |
      (b(s0, 20) << 4) |
      (b(s1, 12) << 3) |
      (b(s0, 12) << 2) |
      (b(s1, 4) << 1) |
      b(s0, 4);
    out[6] =
      (b(s1, 29) << 7) |
      (b(s0, 29) << 6) |
      (b(s1, 21) << 5) |
      (b(s0, 21) << 4) |
      (b(s1, 13) << 3) |
      (b(s0, 13) << 2) |
      (b(s1, 5) << 1) |
      b(s0, 5);
    out[5] =
      (b(s1, 30) << 7) |
      (b(s0, 30) << 6) |
      (b(s1, 22) << 5) |
      (b(s0, 22) << 4) |
      (b(s1, 14) << 3) |
      (b(s0, 14) << 2) |
      (b(s1, 6) << 1) |
      b(s0, 6);
    out[4] =
      (b(s1, 31) << 7) |
      (b(s0, 31) << 6) |
      (b(s1, 23) << 5) |
      (b(s0, 23) << 4) |
      (b(s1, 15) << 3) |
      (b(s0, 15) << 2) |
      (b(s1, 7) << 1) |
      b(s0, 7);
    return out;
  }

  function qrcF(state, key) {
    state = state >>> 0;
    const t1 =
      (((state & 1) << 31) |
        ((state & 0xf8000000) >>> 1) |
        ((state & 0x1f800000) >>> 3) |
        ((state & 0x01f80000) >>> 5) |
        ((state & 0x001f8000) >>> 7)) >>>
      0;
    const t2 =
      (((state & 0x0001f800) << 15) |
        ((state & 0x00001f80) << 13) |
        ((state & 0x000001f8) << 11) |
        ((state & 0x0000001f) << 9) |
        ((state & 0x80000000) >>> 23)) >>>
      0;
    const k0 = ((t1 >>> 24) & 0xff) ^ key[0];
    const k1 = ((t1 >>> 16) & 0xff) ^ key[1];
    const k2 = ((t1 >>> 8) & 0xff) ^ key[2];
    const k3 = ((t2 >>> 24) & 0xff) ^ key[3];
    const k4 = ((t2 >>> 16) & 0xff) ^ key[4];
    const k5 = ((t2 >>> 8) & 0xff) ^ key[5];
    const st =
      ((QRC_SBOX[0][qrcSboxBit(k0 >>> 2)] << 28) |
        (QRC_SBOX[1][qrcSboxBit(((k0 & 0x03) << 4) | (k1 >>> 4))] << 24) |
        (QRC_SBOX[2][qrcSboxBit(((k1 & 0x0f) << 2) | (k2 >>> 6))] << 20) |
        (QRC_SBOX[3][qrcSboxBit(k2 & 0x3f)] << 16) |
        (QRC_SBOX[4][qrcSboxBit(k3 >>> 2)] << 12) |
        (QRC_SBOX[5][qrcSboxBit(((k3 & 0x03) << 4) | (k4 >>> 4))] << 8) |
        (QRC_SBOX[6][qrcSboxBit(((k4 & 0x0f) << 2) | (k5 >>> 6))] << 4) |
        QRC_SBOX[7][qrcSboxBit(k5 & 0x3f)]) >>>
      0;
    const b = (n) => (st >>> n) & 1;
    return (
      ((b(16) << 31) |
        (b(25) << 30) |
        (b(12) << 29) |
        (b(11) << 28) |
        (b(3) << 27) |
        (b(20) << 26) |
        (b(4) << 25) |
        (b(15) << 24) |
        (b(31) << 23) |
        (b(17) << 22) |
        (b(9) << 21) |
        (b(6) << 20) |
        (b(27) << 19) |
        (b(14) << 18) |
        (b(1) << 17) |
        (b(22) << 16) |
        (b(30) << 15) |
        (b(24) << 14) |
        (b(8) << 13) |
        (b(18) << 12) |
        (b(0) << 11) |
        (b(5) << 10) |
        (b(29) << 9) |
        (b(23) << 8) |
        (b(13) << 7) |
        (b(19) << 6) |
        (b(2) << 5) |
        (b(26) << 4) |
        (b(10) << 3) |
        (b(21) << 2) |
        (b(28) << 1) |
        b(7)) >>>
      0
    );
  }

  function qrcCrypt(block, key) {
    let [s0, s1] = qrcInitialPermutation(block);
    for (let idx = 0; idx < 15; idx++) {
      const prev = s1;
      s1 = (qrcF(s1, key[idx]) ^ s0) >>> 0;
      s0 = prev;
    }
    s0 = (qrcF(s1, key[15]) ^ s0) >>> 0;
    return qrcInversePermutation(s0, s1);
  }

  function qrcKeySchedule(key, offset, mode) {
    const schedule = Array.from({ length: 16 }, () => [0, 0, 0, 0, 0, 0]);
    const v0 =
      (key[offset] | (key[offset + 1] << 8) | (key[offset + 2] << 16) | (key[offset + 3] << 24)) >>>
      0;
    const v1 =
      (key[offset + 4] |
        (key[offset + 5] << 8) |
        (key[offset + 6] << 16) |
        (key[offset + 7] << 24)) >>>
      0;
    let c = 0,
      d = 0;
    for (let i = 0; i < QRC_PERM_C.length; i++) {
      const pb = QRC_PERM_C[i];
      const bit = pb < 32 ? (v0 >>> (31 - pb)) & 1 : (v1 >>> (63 - pb)) & 1;
      c = (c | (bit << (31 - i))) >>> 0;
    }
    for (let i = 0; i < QRC_PERM_D.length; i++) {
      const pb = QRC_PERM_D[i];
      const bit = pb < 32 ? (v0 >>> (31 - pb)) & 1 : (v1 >>> (63 - pb)) & 1;
      d = (d | (bit << (31 - i))) >>> 0;
    }
    for (let i = 0; i < 16; i++) {
      const sh = QRC_RND_SHIFT[i];
      c = (((c << sh) | (c >>> (28 - sh))) & 0xfffffff0) >>> 0;
      d = (((d << sh) | (d >>> (28 - sh))) & 0xfffffff0) >>> 0;
      const togen = mode === QRC_DECRYPT ? 15 - i : i;
      for (let j = 0; j < 6; j++) schedule[togen][j] = 0;
      for (let j = 0; j < 24; j++) {
        const bit = (c >>> (31 - QRC_COMPRESSION[j])) & 1;
        schedule[togen][(j / 8) | 0] |= bit << (7 - (j % 8));
      }
      for (let j = 24; j < 48; j++) {
        const bit = (d >>> (31 - (QRC_COMPRESSION[j] - 27))) & 1;
        schedule[togen][(j / 8) | 0] |= bit << (7 - (j % 8));
      }
    }
    return schedule;
  }

  function qrcTripleDesSetup(key, mode) {
    if (mode === QRC_ENCRYPT) {
      return [
        qrcKeySchedule(key, 0, QRC_ENCRYPT),
        qrcKeySchedule(key, 8, QRC_DECRYPT),
        qrcKeySchedule(key, 16, QRC_ENCRYPT),
      ];
    }
    return [
      qrcKeySchedule(key, 16, QRC_DECRYPT),
      qrcKeySchedule(key, 8, QRC_ENCRYPT),
      qrcKeySchedule(key, 0, QRC_DECRYPT),
    ];
  }

  function qrcTripleDesCrypt(block, sched) {
    let d = block;
    for (let i = 0; i < 3; i++) d = qrcCrypt(d, sched[i]);
    return d;
  }

  function qrcDecrypt(encryptedHex) {
    try {
      const raw = new Uint8Array(Buffer.from(encryptedHex, 'hex'));
      if (raw.length === 0 || raw.length % 8 !== 0) throw new Error('invalid block length');
      const sched = qrcTripleDesSetup(QRC_KEY, QRC_DECRYPT);
      const out = new Uint8Array(raw.length);
      const block = new Uint8Array(8);
      for (let i = 0; i < raw.length; i += 8) {
        block.set(raw.subarray(i, i + 8));
        out.set(qrcTripleDesCrypt(block, sched), i);
      }
      return Buffer.from(inflateSync(Buffer.from(out))).toString('utf8');
    } catch {
      throw musicError('PARSE_ERROR', 'QQ 歌词解密失败');
    }
  }

  function qrcToKrc(qrc) {
    if (!qrc || typeof qrc !== 'string') return null;
    let text = qrc;
    const mAttr = text.match(/LyricContent="([\s\S]*?)"/);
    if (mAttr) {
      text = mAttr[1]
        .replace(/&#10;/g, '\n')
        .replace(/&#13;/g, '\r')
        .replace(/&apos;/g, "'")
        .replace(/&quot;/g, '"')
        .replace(/&lt;/g, '<')
        .replace(/&gt;/g, '>')
        .replace(/&amp;/g, '&');
    }
    const out = [];
    for (const raw of text.split(/\r?\n/)) {
      const line = raw.trim();
      if (!line) continue;
      const m = line.match(/^\[(\d+)\s*,\s*(\d+)\](.*)$/);
      if (!m) continue; // 跳过 [ti:]/[ar:] 等元信息与 XML 标签行
      const lineStart = Number(m[1]);
      const lineDur = Number(m[2]);
      let body = '';
      const re = /([^\n(]*?)\((\d+)\s*,\s*(\d+)\)/g;
      let mm;
      while ((mm = re.exec(m[3])) !== null) {
        const syl = mm[1];
        const rel = Math.max(0, Number(mm[2]) - lineStart);
        body += `<${rel},${Number(mm[3])},0>${syl}`;
      }
      if (!/<\d+,\d+/.test(body)) continue;
      out.push(`[${lineStart},${lineDur}]${body}`);
    }
    return out.length > 0 ? out.join('\n') : null;
  }

  const loginSessions = new Map();
  function genSessionId() {
    return randHex(24);
  }
  const QQ_EVENT = { 0: 'success', 66: 'pending', 67: 'scanned', 65: 'expired', 68: 'failed' };
  const WX_EVENT = {
    405: 'success',
    408: 'pending',
    404: 'scanned',
    402: 'expired',
    403: 'failed',
  };

  function parseSetCookies(arr) {
    const out = {};
    for (const c of arr || []) {
      const seg = String(c).split(';')[0];
      const eq = seg.indexOf('=');
      if (eq > 0) out[seg.slice(0, eq).trim()] = seg.slice(eq + 1).trim();
    }
    return out;
  }
  function getSetCookies(res) {
    return res.headers.getSetCookie ? res.headers.getSetCookie() : [];
  }
  function cookieHeader(obj) {
    return Object.entries(obj)
      .map(([k, v]) => `${k}=${v}`)
      .join('; ');
  }

  function mergeCookies(jar, res) {
    for (const c of getSetCookies(res)) {
      const seg = String(c).split(';')[0];
      const eq = seg.indexOf('=');
      if (eq > 0) {
        const k = seg.slice(0, eq).trim();
        const v = seg.slice(eq + 1).trim();
        if (v === '' && jar[k]) continue;
        jar[k] = v;
      }
    }
    return jar;
  }

  function validateAndStoreCredential(item) {
    const code = item.code ?? 0;
    if (code === 0) {
      const cred = item.data || {};
      if (!cred.musickey && !cred.musicKey) throw musicError('UPSTREAM_ERROR', '登录响应缺少凭据');
      const normalized = normalizeCredential(cred);
      setCredential(normalized);
      songs.clear();

      return normalized;
    }
    const map = {
      20271: '验证码错误',
      20276: '需要验证',
      20277: '账号受限',
      20278: '账号受限',
      20279: '登录设备数超限',
      20450: '账号已被封禁',
      104604: '操作过于频繁',
    };
    throw musicError('LOGIN_FAILED', map[code] || `登录失败 code=${code}`, { upstreamCode: code });
  }
  async function prepareQqQr() {
    const url =
      'https://ssl.ptlogin2.qq.com/ptqrshow?appid=716027609&e=2&l=M&s=3&d=72&v=4' +
      `&t=${Math.random()}&daid=383&pt_3rd_aid=100497308`;
    const res = await http(url, { headers: { Referer: 'https://xui.ptlogin2.qq.com/' } });
    const buf = new Uint8Array(await res.arrayBuffer());
    const jar = mergeCookies({}, res);
    if (!jar.qrsig) throw musicError('UPSTREAM_ERROR', '获取 qrsig 失败');
    const sessionId = genSessionId();
    loginSessions.set(sessionId, { method: 'qrcode', variant: 'qq', jar });
    return {
      method: 'qrcode',
      variant: 'qq',
      sessionId,
      qrcode: { imageDataUrl: `data:image/png;base64,${toB64(buf)}` },
      pollIntervalMs: 1500,
    };
  }

  async function pollQqQr(session) {
    const jar = session.jar;
    const ptqrtoken = hash33(jar.qrsig);
    const url =
      'https://ssl.ptlogin2.qq.com/ptqrlogin?u1=' +
      encodeURIComponent('https://graph.qq.com/oauth2.0/login_jump') +
      `&ptqrtoken=${ptqrtoken}&ptredirect=0&h=1&t=1&g=1&from_ui=1&ptlang=2052` +
      `&action=0-0-${Date.now()}&js_ver=20102616&js_type=1&pt_uistyle=40` +
      '&aid=716027609&daid=383&pt_3rd_aid=100497308&has_onekey=1';
    const res = await http(url, {
      headers: { Referer: 'https://xui.ptlogin2.qq.com/', Cookie: cookieHeader(jar) },
    });
    mergeCookies(jar, res);
    const text = await res.text();
    const m = text.match(/ptuiCB\((.*?)\)/s);
    if (!m) throw musicError('PARSE_ERROR', 'QQ 扫码响应格式异常');
    const args = [...m[1].matchAll(/'((?:\\.|[^'])*)'/g)].map((x) => x[1]);
    const code = parseInt(args[0], 10);
    const status = QQ_EVENT[code] || 'failed';
    if (status !== 'success') return { status };
    const sigx = (args[2].match(/[?&]ptsigx=(.+?)&s_url/) || [])[1];
    const uin = (args[2].match(/[?&]uin=(.+?)&service/) || [])[1];
    if (!sigx || !uin) throw musicError('UPSTREAM_ERROR', '解析登录凭据参数失败');
    const cred = await authorizeQqQr(session, uin, sigx);
    return { status: 'success', cred };
  }

  async function authorizeQqQr(session, uin, sigx) {
    const jar = session.jar;
    const checkUrl =
      'https://ssl.ptlogin2.graph.qq.com/check_sig?uin=' +
      `${uin}&pttype=1&service=ptqrlogin&nodirect=0&ptsigx=${sigx}&s_url=` +
      encodeURIComponent('https://graph.qq.com/oauth2.0/login_jump') +
      '&ptlang=2052&ptredirect=100&aid=716027609&daid=383&j_later=0&low_login_hour=0' +
      '&regmaster=0&pt_login_type=3&pt_aid=0&pt_aaid=16&pt_light=0&pt_3rd_aid=100497308';
    const res1 = await http(checkUrl, {
      headers: { Referer: 'https://xui.ptlogin2.qq.com/', Cookie: cookieHeader(jar) },
      redirect: 'manual',
    });
    mergeCookies(jar, res1);
    const pSkey = jar.p_skey;
    if (!pSkey)
      throw musicError('UPSTREAM_ERROR', `获取 p_skey 失败（check_sig status=${res1.status}）`);
    const gtk = hash33(pSkey, 5381);
    const form = new URLSearchParams({
      response_type: 'code',
      client_id: '100497308',
      redirect_uri: 'https://y.qq.com/portal/wx_redirect.html?login_type=1&surl=https://y.qq.com/',
      scope: 'get_user_info,get_app_friends',
      state: 'state',
      switch: '',
      from_ptlogin: '1',
      src: '1',
      update_auth: '1',
      openapi: '1010_1030',
      g_tk: String(gtk),
      auth_time: String(Date.now()),
      ui: randHex(32),
    });
    const res2 = await http('https://graph.qq.com/oauth2.0/authorize', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded',
        Cookie: cookieHeader(jar),
      },
      body: form.toString(),
      redirect: 'manual',
    });
    mergeCookies(jar, res2);
    const loc = res2.headers.get('location') || '';
    const code = (loc.match(/[?&]code=(.+?)(?:&|$)/) || [])[1];
    if (!code)
      throw musicError('UPSTREAM_ERROR', `获取授权 code 失败（authorize status=${res2.status}）`);
    const item = await musicRequest({
      module: 'QQConnectLogin.LoginServer',
      method: 'QQLogin',
      platform: 'android',
      comm: { tmeLoginType: 2 },
      param: { code },
      allowError: true,
    });
    return validateAndStoreCredential(item);
  }
  async function prepareWxQr() {
    const url =
      'https://open.weixin.qq.com/connect/qrconnect?appid=wx48db31d50e334801' +
      '&redirect_uri=' +
      encodeURIComponent(
        'https://y.qq.com/portal/wx_redirect.html?login_type=2&surl=https://y.qq.com/',
      ) +
      '&response_type=code&scope=snsapi_login&state=STATE' +
      '&href=https://y.qq.com/mediastyle/music_v17/src/css/popup_wechat.css%23wechat_redirect';
    const res = await http(url);
    const text = await res.text();
    const uuid = (text.match(/uuid=(.+?)"/) || [])[1];
    if (!uuid) throw musicError('UPSTREAM_ERROR', '获取微信 uuid 失败');
    const imgRes = await http(`https://open.weixin.qq.com/connect/qrcode/${uuid}`, {
      headers: { Referer: 'https://open.weixin.qq.com/connect/qrconnect' },
    });
    const buf = new Uint8Array(await imgRes.arrayBuffer());
    const sessionId = genSessionId();
    loginSessions.set(sessionId, { method: 'qrcode', variant: 'wx', uuid });
    return {
      method: 'qrcode',
      variant: 'wx',
      sessionId,
      qrcode: { imageDataUrl: `data:image/jpeg;base64,${toB64(buf)}` },
      pollIntervalMs: 1500,
    };
  }

  async function pollWxQr(session) {
    const uuid = session.uuid;
    const url = `https://lp.open.weixin.qq.com/connect/l/qrconnect?uuid=${uuid}&_=${Date.now()}`;
    const res = await http(url, { headers: { Referer: 'https://open.weixin.qq.com/' } });
    const text = await res.text();
    const m = text.match(/window\.wx_errcode=(\d+);window\.wx_code='([^']*)'/);
    if (!m) throw musicError('PARSE_ERROR', '微信扫码响应格式异常');
    const code = parseInt(m[1], 10);
    const status = WX_EVENT[code] || 'failed';
    if (status !== 'success') return { status };
    const wxCode = m[2];
    if (!wxCode) throw musicError('UPSTREAM_ERROR', '获取微信 code 失败');
    const item = await musicRequest({
      module: 'music.login.LoginServer',
      method: 'Login',
      platform: 'android',
      comm: { tmeLoginType: 1 },
      param: { code: wxCode, strAppid: 'wx48db31d50e334801' },
      allowError: true,
    });
    return { status: 'success', cred: validateAndStoreCredential(item) };
  }
  async function prepareSms(phone, countryCode) {
    const param = {
      tmeAppid: 'qqmusic',
      areaCode: String(countryCode || 86),
      phoneNo: String(phone),
    };
    const item = await musicRequest({
      module: 'music.login.LoginServer',
      method: 'SendPhoneAuthCode',
      platform: 'android',
      comm: { tmeLoginMethod: 3 },
      param,
      allowError: true,
    });
    const code = item.code ?? 0;
    const sessionId = genSessionId();
    loginSessions.set(sessionId, {
      method: 'sms',
      phone: String(phone),
      countryCode: countryCode || 86,
    });
    if (code === 0) return { method: 'sms', sessionId, pollIntervalMs: 0 };
    if (code === 20276) {
      return { method: 'sms', sessionId, captcha: { url: item.data?.securityURL || '' } };
    }
    if (code === 100001) throw musicError('RATE_LIMITED', '验证码发送过于频繁');
    throw musicError('LOGIN_FAILED', `发送验证码失败 code=${code}`, { upstreamCode: code });
  }

  async function submitSms(session, code) {
    const item = await musicRequest({
      module: 'music.login.LoginServer',
      method: 'Login',
      platform: 'android',
      comm: { tmeLoginMethod: 3, tmeLoginType: 0 },
      param: { code: String(code), loginMode: 1, phoneNo: session.phone },
      allowError: true,
    });
    return validateAndStoreCredential(item);
  }
  function submitCookie(cookieStr) {
    const jar = {};
    for (const seg of String(cookieStr).split(/;\s*/)) {
      const eq = seg.indexOf('=');
      if (eq > 0) jar[seg.slice(0, eq).trim()] = seg.slice(eq + 1).trim();
    }
    const uin = jar.qqmusic_uin || jar.uin || jar.wxuin;
    const key = jar.qqmusic_key || jar.qm_keyst;
    if (!uin || !key) throw musicError('BAD_ARGS', 'cookie 缺少 uin / qm_keyst');
    const musicid = parseInt(String(uin).replace(/\D/g, ''), 10) || 0;
    const cred = {
      musicid,
      str_musicid: String(musicid),
      musickey: key,
      openid: jar.psrf_qqopenid || '',
      refresh_token: jar.psrf_qqrefresh_token || '',
      access_token: jar.psrf_qqaccess_token || '',
      expired_at: parseInt(jar.psrf_access_token_expiresAt || '0', 10) || 0,
      unionid: jar.psrf_qqunionid || '',
      refresh_key: '',
      login_type: parseInt(jar.tmeLoginType || '0', 10) || (String(key).startsWith('W_X') ? 1 : 2),
      refreshedAt: Math.floor(Date.now() / 1000),
    };
    setCredential(cred);
    songs.clear();

    return cred;
  }

  function authStateOf(cred) {
    if (!cred || !cred.musickey) return { loggedIn: false };
    return {
      loggedIn: true,
      user: {
        id: cred.str_musicid || String(cred.musicid || ''),
        name: cred.nick || undefined,
      },
      expiresAt: cred.expired_at || undefined,
    };
  }

  async function prepareLogin(args, context) {
    const method = args?.method || 'qrcode';
    if (method === 'qrcode') {
      const variant = args?.variant || 'qq';
      if (variant === 'wx') return prepareWxQr();
      if (variant === 'qq') return prepareQqQr();
      throw musicError('CAPABILITY_UNSUPPORTED', `不支持的扫码类型: ${variant}`);
    }
    if (method === 'sms') {
      if (!args?.phone) throw musicError('BAD_ARGS', '缺少手机号');
      return prepareSms(args.phone, args.countryCode);
    }
    if (method === 'cookie') {
      return { method: 'cookie', requires: ['cookie'] };
    }
    throw musicError('BAD_ARGS', `不支持的登录方式: ${method}`);
  }

  async function submitLogin(args, context) {
    const method = args?.method || 'qrcode';
    if (method === 'cookie') {
      const cred = submitCookie(args?.cookie || '');
      return { status: 'success', authState: authStateOf(cred) };
    }
    const sessionId = args?.sessionId;
    const session = sessionId ? loginSessions.get(sessionId) : null;
    if (!session) return { status: 'expired', message: '登录会话不存在或已过期' };

    try {
      if (method === 'sms') {
        if (!args?.code) throw musicError('BAD_ARGS', '缺少验证码');
        const cred = await submitSms(session, args.code);
        loginSessions.delete(sessionId);
        return { status: 'success', authState: authStateOf(cred) };
      }
      const r = session.variant === 'wx' ? await pollWxQr(session) : await pollQqQr(session);
      if (r.status === 'success') {
        loginSessions.delete(sessionId);
        return { status: 'success', authState: authStateOf(r.cred) };
      }
      if (r.status === 'expired' || r.status === 'failed') loginSessions.delete(sessionId);
      return { status: r.status };
    } catch (e) {
      if (e?.name === 'MusicPluginError') throw e;
      throw musicError('LOGIN_FAILED', String(e?.message || e));
    }
  }

  async function getAuthState() {
    return authStateOf(getCredential());
  }

  async function logout() {
    currentCredential = {};
    loginSessions.clear();
    songs.clear();
    return { loggedIn: false };
  }
  return {
    search,
    getTrack,
    getPlayback,
    getLyrics,
    getPlaylist,
    getHomeFeed,
    getDiscoverCategories,
    getRecommendedPlaylists,
    getUserPlaylists,
    async startQr(variant) {
      const result = await prepareLogin({ method: 'qrcode', variant });
      return {
        image: result.qrcode.imageDataUrl,
        state: loginSessions.get(result.sessionId),
        expiresIn: 120,
      };
    },
    async pollQr(state) {
      const result = state.variant === 'wx' ? await pollWxQr(state) : await pollQqQr(state);
      if (result.status === 'success') return { status: 'done', ...login() };
      return {
        status:
          result.status === 'pending'
            ? 'waiting'
            : result.status === 'failed'
              ? 'expired'
              : result.status,
      };
    },
    async sendSms(phone) {
      const result = await prepareSms(phone, 86);
      if (result.captcha) throw new Unsupported('短信需要外部安全验证，请改用扫码登录');
      return { state: loginSessions.get(result.sessionId), cooldown: 60 };
    },
    async verifySms(state, code) {
      await submitSms(state, code);
      return login();
    },
    cookie(value) {
      submitCookie(value);
      return login();
    },
    user() {
      return currentCredential.musickey ? login().user : null;
    },
    async refresh() {
      await refreshCredential();
      return currentCredential;
    },
    logout,
    request: musicRequest,
  };
  function login() {
    const state = authStateOf(currentCredential);
    return {
      credential: currentCredential,
      user: { ...state.user, name: currentCredential.nick || state.user?.id || 'QQ 音乐用户' },
    };
  }
}
