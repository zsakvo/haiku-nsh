// Ported from QQMusicApi 108617f (GPL-3.0).
// Centralizes CGI batching, platform comm fields, signing and SDK errors.
// Android's QIMEI and GetSession are cached in memory; account credentials belong to the host.
import {
  AuthRequired,
  AuthExpired,
  VipRequired,
  RateLimited,
  Upstream,
  Expired,
  ParseError,
  NotFound,
} from 'haiku';
import type { Context, HttpOptions, Json } from 'haiku';
import { device, hash33, qimeiRequest, sign } from './crypto.ts';
import type { Credential, Envelope } from './types.ts';

export interface Request {
  module: string;
  method: string;
  param: { [key: string]: Json | undefined };
}
interface Options {
  platform?: 'android' | 'web';
  comm?: { [key: string]: Json };
  sign?: boolean;
  raw?: boolean;
}
export const WEB_UA =
  'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 ' +
  '(KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36';
const fingerprint = device();
let session: Promise<{ q16: string; q36: string; uid: string; sid: string }> | undefined;
let sessionExpires = 0;

export function credential(ctx: Context, required = false): Credential {
  const value = (ctx.credential || {}) as Partial<Credential>;
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
    ...value,
  };
  if (required && (!c.musicid || !c.musickey)) throw new AuthRequired();
  return c;
}
export function cookies(c: Credential): string {
  if (!c.musickey) return '';
  const uin = c.str_musicid || String(c.musicid);
  return `uin=${uin}; qqmusic_uin=${uin}; qm_keyst=${c.musickey}; qqmusic_key=${c.musickey}`;
}
export function checkCode(code = 0): void {
  if (!code) return;
  if ([1000, 104400, 104401].includes(code)) throw new AuthExpired();
  if ([2001, 104604, 100001].includes(code)) throw new RateLimited({ retryAfter: 60 });
  if ([20261, 20271, 20272, 20274, 20277, 20278, 20279, 20450].includes(code))
    throw new AuthRequired(`QQ 登录失败 (${code})`);
  if (code === 104003) throw new VipRequired();
  if (code === 104004) throw new Expired('QQ 播放授权已失效');
  if (code === 404) throw new NotFound();
  throw new Upstream({ message: `QQ upstream code ${code}`, upstreamCode: code });
}
export async function request(
  ctx: Context,
  url: string,
  options: HttpOptions = {},
  post = false,
): Promise<Response> {
  const headers = {
    Referer: 'https://y.qq.com/',
    'User-Agent': WEB_UA,
    ...(cookies(credential(ctx)) ? { Cookie: cookies(credential(ctx)) } : {}),
    ...options.headers,
  };
  const response = await (
    post
      ? ctx.http.post(url, { ...options, headers, throwOnError: false })
      : ctx.http.get(url, { ...options, headers, throwOnError: false })
  ).response();
  if (response.status === 401 || response.status === 403) throw new AuthExpired();
  if (response.status === 429)
    throw new RateLimited({ retryAfter: Number(response.headers.get('retry-after') || 60) });
  if (response.status === 404) throw new NotFound();
  if (!response.ok) throw new Upstream({ status: response.status });
  return response;
}
async function androidSession(ctx: Context) {
  if (!session || Date.now() >= sessionExpires) {
    sessionExpires = Date.now() + 86400000;
    session = (async () => {
      // QQMusicApi initializes QIMEI before Android CGI requests, including search and SMS.
      const res = await request(
        ctx,
        'https://api.tencentmusic.com/tme/trpc/proxy',
        qimeiRequest(fingerprint),
        true,
      );
      const body = (await res.json()) as { data: string };
      const qimei = (JSON.parse(body.data) as { data: { q16: string; q36: string } }).data;
      if (!qimei?.q16 || !qimei.q36) throw new ParseError('Missing QIMEI');
      const values = await send<[{ session: { uid: string | number; sid: string } }]>(
        ctx,
        [
          {
            module: 'music.getSession.session',
            method: 'GetSession',
            param: { uid: '', vkey: 0, caller: 0 },
          },
        ],
        comm(credential(ctx), 'android', { ...qimei, uid: '', sid: '' }),
        'android',
      );
      const value = values[0].session;
      if (!value?.uid || !value.sid) throw new ParseError('Missing QQ Android session');
      return { ...qimei, uid: String(value.uid), sid: value.sid };
    })().catch((error) => {
      session = undefined;
      throw error;
    });
  }
  return session;
}
function comm(
  c: Credential,
  platform: NonNullable<Options['platform']>,
  state?: { q16: string; q36: string; uid: string; sid: string },
): { [key: string]: Json | undefined } {
  const gTk = hash33(c.musickey, 5381);
  if (platform === 'web')
    return {
      ct: 24,
      cv: 4747474,
      platform: 'yqq.json',
      chid: '0',
      uin: c.musicid,
      g_tk: gTk,
      g_tk_new_20200303: gTk,
      format: 'json',
      inCharset: 'utf-8',
      outCharset: 'utf-8',
      notice: 0,
      need_new_code: 1,
    };
  return {
    ct: 11,
    cv: 14090008,
    v: 14090008,
    chid: '10003505',
    tmeAppID: 'qqmusic',
    qq: c.musicid ? String(c.musicid) : undefined,
    authst: c.musickey || undefined,
    tmeLoginType: c.login_type || undefined,
    QIMEI: state?.q16 || '',
    QIMEI36: state?.q36 || '',
    OpenUDID: fingerprint.guid,
    OpenUDID2: fingerprint.guid,
    udid: fingerprint.guid,
    uid: state?.uid,
    sid: state?.sid,
    aid: fingerprint.androidId,
    os_ver: '10',
    phonetype: 'MI 6',
    devicelevel: '29',
    newdevicelevel: '29',
    rom: fingerprint.fingerprint,
  };
}
async function send<T extends unknown[]>(
  ctx: Context,
  requests: Request[],
  common: { [key: string]: Json | undefined },
  platform: NonNullable<Options['platform']>,
  options: Options = {},
): Promise<T> {
  const payload = {
    comm: common,
    ...Object.fromEntries(requests.map((req, i) => [`req_${i}`, req])),
  };
  const text = JSON.stringify(payload);
  const url = `https://u.y.qq.com/cgi-bin/${options.sign ? 'musics' : 'musicu'}.fcg`;
  const response = await request(
    ctx,
    url,
    {
      body: text,
      query: options.sign ? { _: Date.now(), sign: sign(text) } : undefined,
      headers: {
        'Content-Type': 'application/json',
        'User-Agent': platform === 'android' ? 'QQMusic 14090008(android 10)' : WEB_UA,
        Cookie: cookies(credential(ctx)),
      },
    },
    true,
  );
  let body: { code?: number; [key: string]: unknown };
  try {
    body = await response.json();
  } catch {
    throw new ParseError('Invalid QQ CGI JSON');
  }
  if (!body || typeof body !== 'object') throw new ParseError('Missing QQ CGI envelope');
  // QQMusicApi code 2000 requests zzc signing; retry this batch once on musics.fcg.
  if (
    !options.sign &&
    (body.code === 2000 ||
      requests.some((_, i) => (body[`req_${i}`] as Envelope<unknown> | undefined)?.code === 2000))
  )
    return send<T>(ctx, requests, common, platform, { ...options, sign: true });
  checkCode(body.code);
  const results = requests.map((_, i) => {
    const result = body[`req_${i}`] as Envelope<unknown> | undefined;
    if (!result) throw new ParseError(`Missing QQ response req_${i}`);
    if (!options.raw) checkCode(result.code);
    if (!options.raw && result.data === undefined)
      throw new ParseError(`Missing QQ response data req_${i}`);
    return options.raw ? result : result.data;
  });
  return results as T;
}
export async function batch<T extends unknown[]>(
  ctx: Context,
  requests: Request[],
  options: Options = {},
): Promise<T> {
  const platform = options.platform || 'android';
  const c = credential(ctx);
  const common = {
    ...comm(c, platform, platform === 'android' ? await androidSession(ctx) : undefined),
    ...options.comm,
  };
  return send<T>(ctx, requests, common, platform, options);
}
export async function api<T>(
  ctx: Context,
  module: string,
  method: string,
  param: Request['param'] = {},
  options: Options = {},
): Promise<T> {
  return (await batch<[T]>(ctx, [{ module, method, param }], options))[0];
}
