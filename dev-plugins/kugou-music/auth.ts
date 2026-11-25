// Ported from MakcRe/KuGouMusicApi 5d6c9f7 (MIT) and MoeKoeMusic Login.vue.
// Implements Kugou/WeChat QR, SMS, password, and Cookie login plus user info and token refresh.
import { AuthExpired, AuthRequired, ParseError, Unsupported, field } from 'haiku';
import type { Auth, Context, Json, Login, User } from 'haiku';
import { checkEnvelope, credential, ensureDevice, kugou } from './api.ts';
import {
  APPID,
  LITE_T1_IV,
  LITE_T1_KEY,
  LITE_T2_IV,
  LITE_T2_KEY,
  LITE_TOKEN_IV,
  LITE_TOKEN_KEY,
  SRCAPPID,
  WX_APPID,
  WX_SECRET,
  aesDecrypt,
  aesEncrypt,
  md5,
  randomString,
  rsaRawEncrypt,
  sha1,
  signParamsKey,
} from './crypto.ts';
import type { Credential, DeviceState, Envelope, LoginData, UserDetailData, VipDetailData } from './types.ts';

interface QrState {
  variant: 'kugou' | 'wx';
  key: string;
}

interface SmsState {
  mobile: string;
}

function normalizeImage(url?: string): string | undefined {
  if (!url) return undefined;
  return url.replace(/^http:/, 'https:').replaceAll('{size}', '240');
}

function liteDeviceTokens(dev: DeviceState, nowMs: number, prevT1 = ''): { t1: string; t2: string } {
  const t2 = aesEncrypt(
    `${dev.guid}|0f607264fc6318a92b9e13c65db7cd3c|${dev.mac}|${dev.dev}|${nowMs}`,
    { key: LITE_T2_KEY, iv: LITE_T2_IV },
  );
  const t1 = aesEncrypt(prevT1 ? `${prevT1}|${nowMs}` : `|${nowMs}`, {
    key: LITE_T1_KEY,
    iv: LITE_T1_IV,
  });
  return { t1, t2 };
}

function extractCredential(
  data: LoginData | undefined,
  decryptKey?: string,
  previous?: Credential,
): Credential {
  let merged: LoginData = { ...(data || {}) };
  if (merged.secu_params && decryptKey) {
    const decrypted = aesDecrypt<Partial<LoginData>>(merged.secu_params, decryptKey);
    if (typeof decrypted === 'object' && decrypted) {
      merged = { ...merged, ...decrypted };
    } else if (typeof decrypted === 'string' && decrypted) {
      merged.token = decrypted;
    }
  }
  const userid = Number(merged.userid || previous?.userid || 0);
  const token = String(merged.token || previous?.token || '');
  if (!userid || !token) throw new AuthRequired('酷狗登录未返回有效凭据');
  return {
    userid,
    token,
    t1: String(merged.t1 ?? previous?.t1 ?? ''),
    vip_type: Number(merged.vip_type ?? previous?.vip_type ?? 0),
    vip_token: String(merged.vip_token ?? previous?.vip_token ?? ''),
    nickname: String(merged.nickname ?? previous?.nickname ?? ''),
    pic: String(merged.pic ?? previous?.pic ?? ''),
  };
}

async function fetchUser(ctx: Context): Promise<User | null> {
  const c = credential(ctx);
  if (!c.userid || !c.token) return null;
  const nowSec = Math.floor(Date.now() / 1000);
  const pk = rsaRawEncrypt({ token: c.token, clienttime: nowSec }).toUpperCase();
  const detail = await kugou<Envelope<UserDetailData>>(ctx, {
    url: '/v3/get_my_info',
    method: 'POST',
    params: { plat: 1 },
    data: { visit_time: nowSec, usertype: 1, p: pk, userid: c.userid },
    headers: { 'x-router': 'usercenter.kugou.com' },
  });

  let vip: string | undefined;
  try {
    const vipResp = await kugou<Envelope<VipDetailData>>(ctx, {
      baseURL: 'https://kugouvip.kugou.com',
      url: '/v1/get_union_vip',
      method: 'GET',
      params: { busi_type: 'concept' },
    });
    const active = (vipResp.data?.busi_vip || []).find((v) => Number(v.is_vip) === 1);
    if (active) vip = active.product_type === 'svip' ? '超级 VIP' : '概念版 VIP';
  } catch (error) {
    if (error instanceof AuthExpired || (error as Error)?.name === 'AbortError') throw error;
  }

  const info = detail.data || {};
  return {
    id: String(c.userid),
    name: info.nickname || c.nickname || String(c.userid),
    avatar: normalizeImage(info.pic || c.pic),
    vip,
  };
}

async function finalizeLogin(ctx: Context, cred: Credential): Promise<Login> {
  const scoped: Context = { ...ctx, credential: { ...cred } };
  const profile = await fetchUser(scoped);
  return {
    credential: { ...cred },
    user: profile || {
      id: String(cred.userid),
      name: cred.nickname || String(cred.userid),
      avatar: normalizeImage(cred.pic),
    },
  };
}

function parseQrState(value: Json): QrState {
  const s = value as unknown as Partial<QrState>;
  if (!s || (s.variant !== 'kugou' && s.variant !== 'wx') || !s.key) {
    throw new ParseError('Invalid Kugou QR state');
  }
  return { variant: s.variant, key: s.key };
}

async function qrStart(variant: string, ctx: Context) {
  if (variant === 'kugou') {
    const resp = await kugou<Envelope<{ qrcode?: string }>>(ctx, {
      baseURL: 'https://login-user.kugou.com',
      url: '/v2/qrcode',
      method: 'GET',
      encryptType: 'web',
      params: {
        appid: 1001,
        type: 1,
        plat: 4,
        qrcode_txt: `https://h5.kugou.com/apps/loginQRCode/html/index.html?appid=${APPID}&`,
        srcappid: SRCAPPID,
      },
    });
    const key = resp.data?.qrcode;
    if (!key) throw new ParseError('酷狗未返回二维码 key');
    return {
      url: `https://h5.kugou.com/apps/loginQRCode/html/index.html?qrcode=${key}`,
      state: { variant: 'kugou', key },
      expiresIn: 180,
    };
  }

  if (variant === 'wx') {
    const tokenResp = await ctx.http
      .get('https://api.weixin.qq.com/cgi-bin/token', {
        query: { appid: WX_APPID, secret: WX_SECRET, grant_type: 'client_credential' },
      })
      .json<{ access_token?: string }>();
    if (!tokenResp.access_token) throw new ParseError('微信未返回 access_token');

    const ticketResp = await ctx.http
      .get('https://api.weixin.qq.com/cgi-bin/ticket/getticket', {
        query: { access_token: tokenResp.access_token, type: 2 },
      })
      .json<{ errcode?: number; ticket?: string }>();
    if (ticketResp.errcode !== 0 || !ticketResp.ticket) throw new ParseError('微信未返回 sdk_ticket');

    const timestamp = Date.now();
    const noncestr = md5(randomString());
    const signature = sha1(
      `appid=${WX_APPID}&noncestr=${noncestr}&sdk_ticket=${ticketResp.ticket}&timestamp=${timestamp}`,
    );
    const connect = await ctx.http
      .get('https://open.weixin.qq.com/connect/sdk/qrconnect', {
        query: { appid: WX_APPID, noncestr, timestamp, scope: 'snsapi_userinfo', signature },
      })
      .json<{ errcode?: number; uuid?: string }>();
    if (connect.errcode !== 0 || !connect.uuid) throw new ParseError('微信未返回二维码 uuid');

    return {
      url: `https://open.weixin.qq.com/connect/confirm?uuid=${connect.uuid}`,
      state: { variant: 'wx', key: connect.uuid },
      expiresIn: 180,
    };
  }

  throw new Unsupported(`Unsupported Kugou QR variant: ${variant}`);
}

async function loginWithWxCode(ctx: Context, code: string): Promise<Login> {
  const oauth = await ctx.http
    .post('https://api.weixin.qq.com/sns/oauth2/access_token', {
      query: { secret: WX_SECRET, appid: WX_APPID, code, grant_type: 'authorization_code' },
    })
    .json<{ access_token?: string; openid?: string }>();
  if (!oauth.access_token || !oauth.openid) throw new ParseError('微信授权未返回 openid/access_token');

  const dev = await ensureDevice(ctx);
  const nowMs = Date.now();
  const enc = aesEncrypt({ access_token: oauth.access_token });
  const pk = rsaRawEncrypt({ clienttime_ms: nowMs, key: enc.key }).toUpperCase();
  const { t1, t2 } = liteDeviceTokens(dev, nowMs);

  const resp = await kugou<Envelope<LoginData>>(ctx, {
    url: '/v6/login_by_openplat',
    method: 'POST',
    headers: { 'x-router': 'login.user.kugou.com' },
    data: {
      dev: dev.dev,
      force_login: 1,
      partnerid: 36,
      clienttime_ms: nowMs,
      t1,
      t2,
      t3: 'MCwwLDAsMCwwLDAsMCwwLDA=',
      openid: oauth.openid,
      params: enc.str,
      pk,
    },
  });
  return finalizeLogin(ctx, extractCredential(resp.data, enc.key));
}

async function qrPoll(value: Json, ctx: Context) {
  const s = parseQrState(value);
  if (s.variant === 'kugou') {
    const dev = await ensureDevice(ctx);
    const resp = await kugou<
      Envelope<{ status?: number; token?: string; userid?: number; nickname?: string; pic?: string }>
    >(ctx, {
      baseURL: 'https://login-user.kugou.com',
      url: '/v2/get_userinfo_qrcode',
      method: 'GET',
      encryptType: 'web',
      params: { plat: 4, appid: APPID, srcappid: SRCAPPID, qrcode: s.key, dev: dev.dev },
    });
    // Kugou QR status: 0=expired, 1=waiting, 2=scanned, 4=authorized.
    const status = Number(resp.data?.status ?? 1);
    if (status === 1) return { status: 'waiting' as const };
    if (status === 2) return { status: 'scanned' as const };
    if (status === 0) return { status: 'expired' as const };
    if (status === 4) {
      const cred = extractCredential(resp.data);
      return { status: 'done' as const, ...(await finalizeLogin(ctx, cred)) };
    }
    throw new ParseError(`Unexpected Kugou QR status: ${status}`);
  }

  const wxResp = await ctx.http
    .get('https://long.open.weixin.qq.com/connect/l/qrconnect', {
      query: { f: 'json', uuid: s.key },
      timeout: 35000,
    })
    .json<{ wx_errcode?: number; wx_code?: string }>();
  const code = Number(wxResp.wx_errcode ?? 408);
  if (code === 408) return { status: 'waiting' as const };
  if (code === 404) return { status: 'scanned' as const };
  if (code === 402) return { status: 'expired' as const };
  if (code === 403) throw new AuthRequired('用户取消了微信扫码授权');
  if (code === 405 && wxResp.wx_code) {
    return { status: 'done' as const, ...(await loginWithWxCode(ctx, wxResp.wx_code)) };
  }
  throw new ParseError(`Unexpected WeChat QR errcode: ${code}`);
}

async function submitVerifyCode(
  ctx: Context,
  mobile: string,
  code: string,
  userid?: number,
): Promise<{ resp: Envelope<LoginData>; key: string }> {
  const dev = await ensureDevice(ctx);
  const nowMs = Date.now();
  const enc = aesEncrypt({ mobile, code });
  const masked = `${mobile.slice(0, 2)}*****${mobile.slice(10, 11)}`;
  const { t1, t2 } = liteDeviceTokens(dev, nowMs);
  const data: Record<string, Json> = {
    plat: 1,
    support_multi: 1,
    t1,
    t2,
    clienttime_ms: nowMs,
    mobile: masked,
    key: signParamsKey(nowMs),
    pk: rsaRawEncrypt({ clienttime_ms: nowMs, key: enc.key }).toUpperCase(),
    params: enc.str,
    dfid: dev.dfid,
    dev: dev.dev,
    gitversion: '5f0b7c4',
    ...(userid ? { userid } : {}),
  };
  const resp = await kugou<Envelope<LoginData>>(ctx, {
    baseURL: 'https://loginserviceretry.kugou.com',
    url: '/v7/login_by_verifycode',
    method: 'POST',
    data,
    raw: true,
    headers: {
      'support-calm': '1',
      'User-Agent': 'Android16-1070-11440-130-0-LOGIN-wifi',
    },
  });
  return { resp, key: enc.key };
}

export const auth: Auth = {
  qrcode: {
    variants: { kugou: '酷狗', wx: '微信' },
    start: qrStart,
    poll: qrPoll,
  },
  sms: {
    async send(phone, ctx) {
      const mobile = phone.trim();
      if (!/^1\d{10}$/.test(mobile)) throw new ParseError('请输入有效的 11 位中国大陆手机号');
      await kugou<Envelope>(ctx, {
        baseURL: 'https://login.user.kugou.com',
        url: '/v7/send_mobile_code',
        method: 'POST',
        data: { businessid: 5, mobile, plat: 3 },
      });
      return { state: { mobile }, cooldown: 60 };
    },
    async verify(value, code, ctx) {
      const s = value as unknown as Partial<SmsState>;
      if (!s?.mobile) throw new ParseError('Invalid SMS login state');
      let attempt = await submitVerifyCode(ctx, s.mobile, code.trim());
      // Multi-account phones return info_list; pick the primary bound account automatically.
      if (attempt.resp.status !== 1 && attempt.resp.data?.info_list?.length) {
        const chosen = attempt.resp.data.info_list[0].userid;
        attempt = await submitVerifyCode(ctx, s.mobile, code.trim(), chosen);
      }
      checkEnvelope(attempt.resp);
      return finalizeLogin(ctx, extractCredential(attempt.resp.data, attempt.key));
    },
  },
  password: {
    fields: {
      username: field.text('账号 / 邮箱 / 手机号', { placeholder: '请输入酷狗账号或邮箱' }),
      password: field.secret('密码', { placeholder: '请输入密码' }),
    },
    async submit(values, ctx) {
      const username = (values.username || '').trim();
      const password = values.password || '';
      if (!username || !password) throw new ParseError('请输入账号和密码');
      const dev = await ensureDevice(ctx);
      const nowMs = Date.now();
      const enc = aesEncrypt({ pwd: password, code: '', clienttime_ms: nowMs });
      const { t1, t2 } = liteDeviceTokens(dev, nowMs);
      const resp = await kugou<Envelope<LoginData>>(ctx, {
        url: '/v9/login_by_pwd',
        method: 'POST',
        headers: { 'x-router': 'login.user.kugou.com' },
        data: {
          plat: 1,
          support_multi: 1,
          clienttime_ms: nowMs,
          t1,
          t2,
          t3: 'MCwwLDAsMCwwLDAsMCwwLDA=',
          dev: dev.dev,
          username,
          params: enc.str,
          pk: rsaRawEncrypt({ clienttime_ms: nowMs, key: enc.key }).toUpperCase(),
        },
      });
      return finalizeLogin(ctx, extractCredential(resp.data, enc.key));
    },
  },
  cookie: {
    async submit(text, ctx) {
      const pairs = Object.fromEntries(
        text
          .split(';')
          .map((entry) => {
            const at = entry.indexOf('=');
            return at < 1 ? ['', ''] : [entry.slice(0, at).trim(), entry.slice(at + 1).trim()];
          })
          .filter(([k, v]) => k && v),
      );
      const userid = Number(pairs.userid || pairs.KugooID || 0);
      const token = pairs.token || pairs.t || '';
      if (!userid || !token) throw new ParseError('Cookie 缺少 userid 或 token');
      const cred = extractCredential({
        userid,
        token,
        t1: pairs.t1 || '',
        vip_type: Number(pairs.vip_type || 0),
        vip_token: pairs.vip_token || '',
      });
      return finalizeLogin(ctx, cred);
    },
  },
  user: fetchUser,
  async refresh(ctx) {
    const c = credential(ctx, true);
    const dev = await ensureDevice(ctx);
    const nowMs = Date.now();
    const p3 = aesEncrypt(
      { clienttime: Math.floor(nowMs / 1000), token: c.token },
      { key: LITE_TOKEN_KEY, iv: LITE_TOKEN_IV },
    );
    const enc = aesEncrypt({});
    const pk = rsaRawEncrypt({ clienttime_ms: nowMs, key: enc.key });
    const { t1, t2 } = liteDeviceTokens(dev, nowMs, c.t1);
    const resp = await kugou<Envelope<LoginData>>(ctx, {
      baseURL: 'https://login.user.kugou.com',
      url: '/v5/login_by_token',
      method: 'POST',
      data: {
        dfid: dev.dfid,
        p3,
        plat: 1,
        t1,
        t2,
        t3: 'MCwwLDAsMCwwLDAsMCwwLDA=',
        pk,
        params: enc.str,
        userid: c.userid,
        clienttime_ms: nowMs,
        dev: dev.dev,
      },
    });
    try {
      return { ...extractCredential(resp.data, enc.key, c) };
    } catch {
      throw new AuthExpired('酷狗凭据刷新失败，请重新登录');
    }
  },
};
