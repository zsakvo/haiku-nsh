// Ported from MakcRe/KuGouMusicApi 5d6c9f7 (MIT) for the Kugou Lite (概念版) protocol.
// Centralizes gateway requests, Lite signing, device registration (dfid/mid), and error mapping.
import { AuthExpired, AuthRequired, NotFound, ParseError, RateLimited, Upstream } from 'haiku';
import type { Context, Json } from 'haiku';
import {
  APPID,
  CLIENTVER,
  calculateMid,
  deviceAesDecrypt,
  deviceAesEncrypt,
  md5,
  randomGuid,
  randomString,
  rsaPkcs1Encrypt,
  signKey,
  signatureAndroidParams,
  signatureWebParams,
} from './crypto.ts';
import type { Credential, DeviceState, Envelope } from './types.ts';

export const GATEWAY_BASE = 'https://gateway.kugou.com';
const DEVICE_KEY = 'kugou.device.lite.v1';
const DEFAULT_UA = 'Android15-1070-11083-46-0-DiscoveryDRADProtocol-wifi';

let pendingDevice: Promise<DeviceState> | undefined;

export interface RequestOptions {
  baseURL?: string;
  url: string;
  method?: 'GET' | 'POST';
  params?: Record<string, string | number | boolean | undefined>;
  data?: Json | string;
  headers?: Record<string, string>;
  encryptType?: 'android' | 'web';
  encryptKey?: boolean;
  clearDefaultParams?: boolean;
  clearDefaultHeaders?: boolean;
  notSignature?: boolean;
  skipDeviceRegister?: boolean;
  raw?: boolean;
  timeout?: number;
}

export function credential(ctx: Context, required = false): Credential {
  const raw = (ctx.credential || {}) as Partial<Credential>;
  const c: Credential = {
    userid: Number(raw.userid || 0),
    token: String(raw.token || ''),
    t1: String(raw.t1 || ''),
    vip_type: Number(raw.vip_type || 0),
    vip_token: String(raw.vip_token || ''),
    nickname: String(raw.nickname || ''),
    pic: String(raw.pic || ''),
  };
  if (required && (!c.userid || !c.token)) throw new AuthRequired('请先登录酷狗音乐账号');
  return c;
}

/** Map Kugou upstream envelope status/error codes to Haiku SDK error classes. */
export function checkEnvelope(body: Envelope, ssaCode?: string | null): void {
  if (ssaCode) {
    throw new RateLimited({ retryAfter: 60, message: '酷狗触发安全风控验证，请稍后重试' });
  }
  const code = Number(body.error_code ?? body.errcode ?? 0);
  // 20018/20019/20020/20021/20031/20032 or status=2 indicate expired/invalid login token.
  if (body.status === 2 || [20018, 20019, 20020, 20021, 20031, 20032].includes(code)) {
    throw new AuthExpired(body.errmsg || body.error || '酷狗登录已失效，请重新登录');
  }
  if ([20028, 20033].includes(code)) {
    throw new RateLimited({ retryAfter: 60, message: body.errmsg || body.error || '酷狗风控限制' });
  }
  if (body.status === 0 || (code !== 0 && body.status !== 200)) {
    throw new Upstream({
      message: body.errmsg || body.error || `Kugou upstream error (status=${body.status}, code=${code})`,
      upstreamCode: code || body.status || 0,
    });
  }
}

function readCachedDevice(ctx: Context): DeviceState | null {
  const saved = ctx.storage.get(DEVICE_KEY) as Partial<DeviceState> | null;
  if (
    saved &&
    typeof saved.guid === 'string' &&
    saved.guid.length > 0 &&
    typeof saved.mid === 'string' &&
    saved.mid.length > 0 &&
    typeof saved.dev === 'string' &&
    saved.dev.length > 0 &&
    typeof saved.mac === 'string' &&
    typeof saved.dfid === 'string' &&
    saved.dfid.length > 0
  ) {
    return saved as DeviceState;
  }
  return null;
}

/** Obtain or register a persistent Lite device fingerprint (guid, mid, dev, dfid). */
export async function ensureDevice(ctx: Context, register = true): Promise<DeviceState> {
  const cached = readCachedDevice(ctx);
  if (cached && (!register || cached.dfid !== '-')) return cached;

  const base: DeviceState = cached || {
    guid: md5(randomGuid()),
    mid: '',
    dev: randomString(10),
    mac: '02:00:00:00:00:00',
    dfid: '-',
  };
  if (!base.mid) base.mid = calculateMid(base.guid);

  if (!register) return base;
  if (pendingDevice) return pendingDevice;

  pendingDevice = (async () => {
    const c = credential(ctx);
    const payload = {
      availableRamSize: 4983533568,
      availableRomSize: 48114719,
      availableSDSize: 48114717,
      basebandVer: '',
      batteryLevel: 100,
      batteryStatus: 3,
      brand: 'Redmi',
      buildSerial: 'unknown',
      device: 'marble',
      imei: base.guid,
      imsi: '',
      manufacturer: 'Xiaomi',
      uuid: base.guid,
      accelerometer: false,
      accelerometerValue: '',
      gravity: false,
      gravityValue: '',
      gyroscope: false,
      gyroscopeValue: '',
      light: false,
      lightValue: '',
      magnetic: false,
      magneticValue: '',
      orientation: false,
      orientationValue: '',
      pressure: false,
      pressureValue: '',
      step_counter: false,
      step_counterValue: '',
      temperature: false,
      temperatureValue: '',
    };
    const enc = deviceAesEncrypt(payload);
    const p = rsaPkcs1Encrypt({ aes: enc.key, uid: c.userid, token: c.token });
    const clienttime = Math.floor(Date.now() / 1000);
    const query: Record<string, string | number | boolean | undefined> = {
      dfid: '-',
      mid: base.mid,
      uuid: '-',
      appid: APPID,
      clientver: CLIENTVER,
      clienttime,
      part: 1,
      platid: 1,
      p,
      ...(c.token ? { token: c.token } : {}),
      ...(c.userid ? { userid: c.userid } : {}),
    };
    query.signature = signatureAndroidParams(query, enc.str);
    try {
      const response = await ctx.http
        .post('https://userservice.kugou.com/risk/v3/r_register_dev', {
          query,
          body: enc.str,
          headers: {
            'User-Agent': DEFAULT_UA,
            dfid: '-',
            mid: base.mid,
            clienttime: String(clienttime),
            'kg-rc': '1',
            'kg-thash': '5d816a0',
            'kg-rec': '1',
            'kg-rf': 'B9EDA08A64250DEFFBCADDEE00F8F25F',
          },
          throwOnError: false,
        })
        .response();
      if (response.ok) {
        const rawBytes = new Uint8Array(await response.arrayBuffer());
        let decoded: Envelope<{ dfid?: string }> | string = '';
        try {
          decoded = deviceAesDecrypt<Envelope<{ dfid?: string }>>(rawBytes, enc.key);
        } catch {
          try {
            decoded = JSON.parse(new TextDecoder().decode(rawBytes)) as Envelope<{ dfid?: string }>;
          } catch {
            decoded = '';
          }
        }
        if (typeof decoded === 'object' && decoded?.data?.dfid) {
          base.dfid = decoded.data.dfid;
        }
      }
    } catch (error) {
      // Allow abort signals to propagate; degrade to dfid='-' only for non-fatal registration errors.
      if ((error as Error)?.name === 'AbortError') throw error;
      ctx.log.warn('Kugou device registration skipped, continuing with fallback dfid');
    }
    ctx.storage.set(DEVICE_KEY, { ...base });
    return base;
  })().finally(() => {
    pendingDevice = undefined;
  });

  return pendingDevice;
}

/** Send a signed request to the Kugou Lite gateway or auxiliary endpoint. */
export async function kugou<T = Envelope>(ctx: Context, options: RequestOptions): Promise<T> {
  const dev = await ensureDevice(ctx, !options.skipDeviceRegister);
  const c = credential(ctx);
  const now = Math.floor(Date.now() / 1000);
  const method = options.method || 'GET';

  const params: Record<string, string | number | boolean | undefined> = options.clearDefaultParams
    ? { ...(options.params || {}) }
    : {
        dfid: dev.dfid,
        mid: dev.mid,
        uuid: '-',
        appid: APPID,
        clientver: CLIENTVER,
        clienttime: now,
        ...(c.token ? { token: c.token } : {}),
        ...(c.userid ? { userid: c.userid } : {}),
        ...(options.params || {}),
      };

  if (options.encryptKey) {
    params.key = signKey(
      String(params.hash || ''),
      String(params.mid ?? dev.mid),
      Number(params.userid ?? c.userid ?? 0),
      Number(params.appid ?? APPID),
    );
  }

  const bodyText =
    options.data === undefined
      ? ''
      : typeof options.data === 'string'
        ? options.data
        : JSON.stringify(options.data);

  if (!options.notSignature && !params.signature) {
    params.signature =
      options.encryptType === 'web'
        ? signatureWebParams(params, bodyText)
        : signatureAndroidParams(params, bodyText);
  }

  const headers: Record<string, string> = options.clearDefaultHeaders
    ? { ...(options.headers || {}) }
    : {
        'User-Agent': DEFAULT_UA,
        dfid: dev.dfid,
        mid: dev.mid,
        clienttime: String(params.clienttime ?? now),
        'kg-rc': '1',
        'kg-thash': '5d816a0',
        'kg-rec': '1',
        'kg-rf': 'B9EDA08A64250DEFFBCADDEE00F8F25F',
        ...(options.data !== undefined && typeof options.data !== 'string'
          ? { 'Content-Type': 'application/json' }
          : {}),
        ...(options.headers || {}),
      };

  const fullUrl = `${options.baseURL || GATEWAY_BASE}${options.url}`;
  const req =
    method === 'POST'
      ? ctx.http.post(fullUrl, {
          query: params,
          body: bodyText || undefined,
          headers,
          timeout: options.timeout,
          throwOnError: false,
        })
      : ctx.http.get(fullUrl, {
          query: params,
          headers,
          timeout: options.timeout,
          throwOnError: false,
        });

  const response = await req.response();
  if (response.status === 401 || response.status === 403) throw new AuthExpired();
  if (response.status === 429) {
    throw new RateLimited({ retryAfter: Number(response.headers.get('retry-after') || 60) });
  }
  if (response.status === 404) throw new NotFound();
  if (!response.ok) throw new Upstream({ status: response.status, message: `HTTP ${response.status}` });

  let body: T;
  try {
    body = (await response.json()) as T;
  } catch {
    throw new ParseError(`Invalid Kugou JSON from ${options.url}`);
  }
  if (!body || typeof body !== 'object') throw new ParseError(`Empty Kugou response from ${options.url}`);

  if (!options.raw) {
    const ssaCode = response.headers.get('ssa-code') || response.headers.get('SSA-CODE');
    checkEnvelope(body as Envelope, ssaCode);
  }
  return body;
}
