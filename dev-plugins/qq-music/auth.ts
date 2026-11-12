// Ported from QQMusicApi 108617f (GPL-3.0).
// Implements QQ/WeChat QR, SMS and Cookie login plus host-owned credential refresh/logout.
// QR state is serializable and contains only the temporary upstream login identifiers.
import { AuthRequired, AuthExpired, ParseError, RateLimited, Unsupported, Upstream } from 'haiku';
import type { Auth, Context, Json, Login, User } from 'haiku';
import { api, credential, checkCode, request, WEB_UA } from './api.ts';
import { Buffer } from 'buffer';
import { hash33, randomHex } from './crypto.ts';
import type { Credential, LoginResult, LoginUser } from './types.ts';

const LOGIN = 'music.login.LoginServer';
const REFERER = 'https://xui.ptlogin2.qq.com/';
interface QRState {
  variant: string;
  identifier: string;
}

function normalize(value: LoginResult, previous?: Credential): Credential {
  const c = {
    musicid: 0,
    musickey: '',
    str_musicid: '',
    login_type: 0,
    openid: '',
    access_token: '',
    refresh_token: '',
    expired_at: 0,
    unionid: '',
    refresh_key: '',
    encrypt_uin: '',
    ...previous,
    ...value,
  };
  c.login_type = value.loginType ?? value.login_type ?? (c.musickey.startsWith('W_X') ? 1 : 2);
  c.encrypt_uin = value.encryptUin ?? c.encrypt_uin;
  if (!c.musicid || !c.musickey) throw new AuthRequired('QQ 登录未返回有效凭据');
  return {
    musicid: c.musicid,
    musickey: c.musickey,
    str_musicid: c.str_musicid,
    login_type: c.login_type,
    openid: c.openid,
    access_token: c.access_token,
    refresh_token: c.refresh_token,
    expired_at: c.expired_at,
    unionid: c.unionid,
    refresh_key: c.refresh_key,
    encrypt_uin: c.encrypt_uin,
  };
}
async function login(
  ctx: Context,
  param: { [key: string]: Json },
  loginType: number,
  qq = false,
): Promise<Login> {
  // QQMusicApi: login type 1=WeChat, 2=QQ; SMS authorizes with type 0 and tmeLoginMethod=3.
  const value = await api<LoginResult>(
    ctx,
    qq ? 'QQConnectLogin.LoginServer' : LOGIN,
    qq ? 'QQLogin' : 'Login',
    param,
    { comm: { tmeLoginType: loginType, ...(loginType === 0 ? { tmeLoginMethod: 3 } : {}) } },
  );
  const c = normalize(value);
  return {
    credential: { ...c },
    user: (await user({ ...ctx, credential: { ...c } })) || undefined,
  };
}
async function user(ctx: Context): Promise<User | null> {
  const c = credential(ctx);
  if (!c.musicid || !c.musickey) return null;
  const value = await api<LoginUser & { userInfo?: LoginUser }>(
    ctx,
    'music.UserInfo.userInfoServer',
    'GetLoginUserInfo',
  );
  const info = value.userInfo || value;
  return {
    id: c.str_musicid || String(c.musicid),
    name: info.nick || info.nickname || String(c.musicid),
    avatar: info.headurl || `https://q1.qlogo.cn/g?b=qq&nk=${c.musicid}&s=100`,
  };
}
function state(value: Json): QRState {
  const s = value as unknown as QRState;
  if (!s || !['qq', 'wx'].includes(s.variant) || !s.identifier)
    throw new ParseError('Invalid QR state');
  return s;
}
function setCookies(response: Response): Record<string, string> {
  const lines = response.headers.getSetCookie();
  // ptlogin also clears the same names on sibling domains (empty value, past Expires),
  // in any order; skip those so they cannot overwrite the real p_skey/qrsig.
  return Object.fromEntries(
    lines
      .map((line) => {
        const first = line.split(';')[0];
        const at = first.indexOf('=');
        return [first.slice(0, at).trim(), first.slice(at + 1).trim()];
      })
      .filter(([name, value]) => name && value),
  );
}
async function authorizationResponse(
  ctx: Context,
  url: string,
  jar: Record<string, string>,
  form?: Record<string, string>,
): Promise<Response> {
  // These two QQ authorization steps need the intermediate 302 Set-Cookie/Location; ctx.http cannot disable redirects.
  const response = await fetch(url, {
    method: form ? 'POST' : 'GET',
    redirect: 'manual',
    signal: ctx.signal,
    headers: {
      Referer: REFERER,
      'User-Agent': WEB_UA,
      Cookie: Object.entries(jar)
        .map(([k, v]) => `${k}=${v}`)
        .join('; '),
      ...(form ? { 'Content-Type': 'application/x-www-form-urlencoded' } : {}),
    },
    body: form ? new URLSearchParams(form).toString() : undefined,
  });
  if (response.status !== 302 && !response.ok) throw new Upstream({ status: response.status });
  Object.assign(jar, setCookies(response));
  return response;
}
async function authorizeQQ(ctx: Context, redirect: string): Promise<Login> {
  const callback = new URL(redirect);
  const uin = callback.searchParams.get('uin');
  const sigx = callback.searchParams.get('ptsigx');
  if (!uin || !sigx) throw new ParseError('QQ callback missing uin/ptsigx');
  const params = new URLSearchParams({
    uin,
    pttype: '1',
    service: 'ptqrlogin',
    nodirect: '0',
    ptsigx: sigx,
    s_url: 'https://graph.qq.com/oauth2.0/login_jump',
    ptlang: '2052',
    ptredirect: '100',
    aid: '716027609',
    daid: '383',
    j_later: '0',
    low_login_hour: '0',
    regmaster: '0',
    pt_login_type: '3',
    pt_aid: '0',
    pt_aaid: '16',
    pt_light: '0',
    pt_3rd_aid: '100497308',
  });
  const jar: Record<string, string> = {};
  await authorizationResponse(ctx, `https://ssl.ptlogin2.graph.qq.com/check_sig?${params}`, jar);
  if (!jar.p_skey) throw new ParseError('QQ check_sig missing p_skey');
  const response = await authorizationResponse(
    ctx,
    'https://graph.qq.com/oauth2.0/authorize',
    jar,
    {
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
      g_tk: String(hash33(jar.p_skey, 5381)),
      auth_time: String(Math.floor(Date.now() / 1000) * 1000),
      ui: randomHex(16),
    },
  );
  const location = response.headers.get('location');
  const code = location && new URL(location).searchParams.get('code');
  if (!code) throw new ParseError('QQ OAuth missing code');
  return login(ctx, { code }, 2, true);
}
async function qrStart(variant: string, ctx: Context) {
  let response: Response;
  let identifier: string;
  if (variant === 'qq') {
    response = await request(ctx, 'https://ssl.ptlogin2.qq.com/ptqrshow', {
      headers: { Referer: REFERER, Cookie: '' },
      query: {
        appid: '716027609',
        e: 2,
        l: 'M',
        s: 3,
        d: 72,
        v: 4,
        t: Math.random(),
        daid: '383',
        pt_3rd_aid: '100497308',
      },
    });
    identifier = setCookies(response).qrsig;
  } else if (variant === 'wx') {
    const page = await request(ctx, 'https://open.weixin.qq.com/connect/qrconnect', {
      headers: { Cookie: '' },
      query: {
        appid: 'wx48db31d50e334801',
        redirect_uri:
          'https://y.qq.com/portal/wx_redirect.html?login_type=2&surl=https://y.qq.com/',
        response_type: 'code',
        scope: 'snsapi_login',
        state: 'STATE',
        href: 'https://y.qq.com/mediastyle/music_v17/src/css/popup_wechat.css#wechat_redirect',
      },
    });
    const html = await page.text();
    identifier = html.match(/uuid=([^"&]+)/)?.[1] || '';
    if (!identifier) throw new ParseError('WeChat QR missing uuid');
    response = await request(
      ctx,
      `https://open.weixin.qq.com/connect/qrcode/${encodeURIComponent(identifier)}`,
      { headers: { Cookie: '', Referer: 'https://open.weixin.qq.com/connect/qrconnect' } },
    );
  } else throw new Unsupported(`Unknown QQ login variant ${variant}`);
  if (!identifier) throw new ParseError('QQ QR missing identifier');
  return {
    image: `data:image/${variant === 'qq' ? 'png' : 'jpeg'};base64,${Buffer.from(new Uint8Array(await response.arrayBuffer())).toString('base64')}`,
    state: { variant, identifier },
    expiresIn: 180,
  };
}
async function qrPoll(value: Json, ctx: Context) {
  const s = state(value);
  let code: number;
  let callback = '';
  if (s.variant === 'qq') {
    const response = await request(ctx, 'https://ssl.ptlogin2.qq.com/ptqrlogin', {
      headers: { Referer: REFERER, Cookie: `qrsig=${s.identifier}` },
      query: {
        u1: 'https://graph.qq.com/oauth2.0/login_jump',
        ptqrtoken: hash33(s.identifier),
        ptredirect: 0,
        h: 1,
        t: 1,
        g: 1,
        from_ui: 1,
        ptlang: 2052,
        action: `0-0-${Date.now()}`,
        js_ver: '20102616',
        js_type: 1,
        pt_uistyle: 40,
        aid: '716027609',
        daid: '383',
        pt_3rd_aid: '100497308',
        has_onekey: 1,
      },
    });
    const values = [...(await response.text()).matchAll(/'((?:\\.|[^'])*)'/g)].map((m) => m[1]);
    code = Number(values[0]);
    callback = values[2] || '';
  } else {
    const response = await request(ctx, 'https://lp.open.weixin.qq.com/connect/l/qrconnect', {
      headers: { Referer: 'https://open.weixin.qq.com/', Cookie: '' },
      timeout: 35000,
      query: { uuid: s.identifier, _: Date.now() },
    });
    const text = await response.text();
    const match = text.match(/window\.wx_errcode=(\d+);window\.wx_code='([^']*)'/);
    if (!match) throw new ParseError('Invalid WeChat QR status');
    code = Number(match[1]);
    callback = match[2];
  }
  // QQ: 66/67/65; WeChat: 408/404/402 mean waiting/scanned/expired.
  if ([66, 408].includes(code)) return { status: 'waiting' as const };
  if ([67, 404].includes(code)) return { status: 'scanned' as const };
  if ([65, 402].includes(code)) return { status: 'expired' as const };
  if ([68, 403].includes(code)) throw new AuthRequired('用户拒绝扫码授权');
  if (![0, 405].includes(code) || !callback) throw new ParseError('Invalid QR callback');
  return {
    status: 'done' as const,
    ...(await (s.variant === 'qq'
      ? authorizeQQ(ctx, callback)
      : login(ctx, { code: callback, strAppid: 'wx48db31d50e334801' }, 1))),
  };
}
export const auth: Auth = {
  qrcode: { variants: { qq: 'QQ', wx: '微信' }, start: qrStart, poll: qrPoll },
  sms: {
    async send(phone, ctx) {
      const field = /^\d+$/.test(phone) ? 'phoneNo' : 'encryptedPhoneNo';
      const result = await api<{ code: number; data?: { securityURL?: string } }>(
        ctx,
        LOGIN,
        'SendPhoneAuthCode',
        { tmeAppid: 'qqmusic', areaCode: '86', [field]: phone },
        { comm: { tmeLoginMethod: 3 }, raw: true },
      );
      // 20276 requires interactive captcha; the SDK SMS flow supports code entry only.
      if (result.code === 20276) throw new Unsupported('QQ 需要额外安全验证，请改用扫码登录');
      if (result.code === 100001) throw new RateLimited({ retryAfter: 60 });
      checkCode(result.code);
      return { state: { phone, field }, cooldown: 60 };
    },
    async verify(value, code, ctx) {
      const s = value as { phone?: string; field?: string };
      if (!s?.phone || !['phoneNo', 'encryptedPhoneNo'].includes(s.field || ''))
        throw new ParseError('Invalid SMS state');
      return login(ctx, { [s.field!]: s.phone, code, loginMode: 1 }, 0);
    },
  },
  cookie: {
    async submit(text, ctx) {
      const values = Object.fromEntries(
        text.split(';').map((v) => {
          const at = v.indexOf('=');
          return [v.slice(0, at).trim(), v.slice(at + 1).trim()];
        }),
      );
      const id = (values.qqmusic_uin || values.uin || '').replace(/^o0*/, '');
      const key = values.qm_keyst || values.qqmusic_key;
      if (!/^\d+$/.test(id) || !key) throw new ParseError('Cookie 缺少 uin / qm_keyst');
      const c = normalize({
        musicid: Number(id),
        musickey: key,
        str_musicid: id,
        openid: values.openid || '',
        access_token: values.access_token || '',
        refresh_token: values.refresh_token || '',
        expired_at: Number(values.expired_at || 0),
        refresh_key: values.refresh_key || '',
        unionid: values.unionid || '',
        encrypt_uin: values.encrypt_uin || values.encryptUin || '',
      });
      const scoped = { ...ctx, credential: { ...c } };
      const account = await user(scoped);
      if (!c.encrypt_uin) {
        // Cookie lacks encryptUin; an owned playlist's creator supplies it for favorites and dirid 201.
        const lists = await api<{ v_playlist: { tid: number; invalid?: boolean }[] }>(
          scoped,
          'music.musicasset.PlaylistBaseRead',
          'GetPlaylistByUin',
          { uin: id },
        );
        const owned = lists.v_playlist.find((p) => p.tid && !p.invalid);
        if (owned) {
          const value = await api<{ dirinfo: { creator: { encrypt_uin?: string } } }>(
            scoped,
            'music.srfDissInfo.DissInfo',
            'CgiGetDiss',
            {
              disstid: owned.tid,
              dirid: 0,
              tag: 0,
              song_begin: 0,
              song_num: 1,
              userinfo: 1,
              orderlist: 1,
              onlysonglist: 0,
            },
          );
          c.encrypt_uin = value.dirinfo.creator.encrypt_uin || '';
        }
      }
      return { credential: { ...c }, user: account || undefined };
    },
  },
  user,
  async refresh(ctx) {
    const c = credential(ctx, true);
    const common = {
      openid: c.openid,
      refresh_token: c.refresh_token,
      musickey: c.musickey,
      refresh_key: c.refresh_key,
      loginMode: 2,
    };
    const param =
      c.login_type === 1
        ? { ...common, str_musicid: c.str_musicid || String(c.musicid), unionid: c.unionid }
        : {
            ...common,
            access_token: c.access_token,
            expired_in: c.expired_at,
            musicid: c.musicid,
            ...(c.login_type === 2
              ? {}
              : { str_musicid: c.str_musicid || String(c.musicid), unionid: c.unionid }),
          };
    const value = await api<LoginResult>(ctx, LOGIN, 'Login', param, {
      comm: { tmeLoginType: c.login_type },
    });
    try {
      return { ...normalize(value, c) };
    } catch {
      throw new AuthExpired('QQ 凭据刷新失败');
    }
  },
  async logout(ctx) {
    credential(ctx, true);
    await api(ctx, LOGIN, 'Logout');
  },
};
