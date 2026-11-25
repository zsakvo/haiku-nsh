// Ported from MakcRe/KuGouMusicApi 5d6c9f7 (MIT) for the Kugou Lite (概念版) protocol.
// Provides MD5/SHA1 signing, AES-CBC/RSA encryption, device MID derivation, and KRC lyric decoding.
import { createHash, randomBytes, createCipheriv, createDecipheriv, publicEncrypt } from 'crypto';
import { Buffer } from 'buffer';
import { inflateSync } from 'zlib';

// Kugou Lite (概念版) constants from util/config.json and util/helper.js.
export const APPID = 3116;
export const CLIENTVER = 11440;
export const APIVER = 20;
export const SRCAPPID = 2919;
export const WX_APPID = 'wx72b795aca60ad321';
export const WX_SECRET = '33e486041e5e25729a4e3d2da7502f9a';

const ANDROID_SIGN_SALT = 'LnT6xpN3khm36zse0QzvmgTZ3waWdRSA';
const WEB_SIGN_SALT = 'NVPh5oo715z5DIWAeQlhMDsWXXQV4hwt';
const SIGN_KEY_SALT = '185672dd44712f60bb1736df5a377e82';

export const LITE_TOKEN_KEY = 'c24f74ca2820225badc01946dba4fdf7';
export const LITE_TOKEN_IV = 'adc01946dba4fdf7';
export const LITE_T1_KEY = '5e4ef500e9597fe004bd09a46d8add98';
export const LITE_T1_IV = '04bd09a46d8add98';
export const LITE_T2_KEY = 'fd14b35e3f81af3817a20ae7adae7020';
export const LITE_T2_IV = '17a20ae7adae7020';

const PUBLIC_LITE_RSA_PEM = `-----BEGIN PUBLIC KEY-----
MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQDECi0Np2UR87scwrvTr72L6oO0
1rBbbBPriSDFPxr3Z5syug0O24QyQO8bg27+0+4kBzTBTBOZ/WWU0WryL1JSXRTX
LgFVxtzIY41Pe7lPOgsfTCn5kZcvKhYKJesKnnJDNr5/abvTGf+rHG3YRwsCHcQ0
8/q6ifSioBszvb3QiwIDAQAB
-----END PUBLIC KEY-----`;

// Modulus n (1024-bit, 128 bytes) and exponent e extracted from PUBLIC_LITE_RSA_PEM for raw RSA.
const PUBLIC_LITE_RSA_N = BigInt(
  '0xc40a2d0da76511f3bb1cc2bbd3afbd8bea83b4d6b05b6c13eb8920c53f1af767' +
    '9b32ba0d0edb843240ef1b836efed3ee240734c14c1399fd6594d16af22f5252' +
    '5d14d72e0155c6dcc8638d4f7bb94f3a0b1f4c29f991972f2a160a25eb0a9e72' +
    '4336be7f69bbd319ffab1c6dd8470b021dc434f3faba89f4a2a01b33bdbdd08b',
);
const PUBLIC_LITE_RSA_E = 65537n;
const RSA_KEY_BYTES = 128;

// 16-byte cyclic XOR key for Kugou's krc1 lyric container.
const KRC_XOR_KEY = [64, 71, 97, 119, 94, 50, 116, 71, 81, 54, 49, 45, 206, 210, 110, 105];

function concatBytes(a: Uint8Array, b: Uint8Array): Uint8Array {
  const out = new Uint8Array(a.length + b.length);
  out.set(a, 0);
  out.set(b, a.length);
  return out;
}

function toUtf8Bytes(data: string | Uint8Array | object): Uint8Array {
  if (data instanceof Uint8Array) return data;
  return Buffer.from(typeof data === 'string' ? data : JSON.stringify(data), 'utf8');
}

function modPow(base: bigint, exp: bigint, mod: bigint): bigint {
  let result = 1n;
  let b = base % mod;
  let e = exp;
  while (e > 0n) {
    if (e & 1n) result = (result * b) % mod;
    b = (b * b) % mod;
    e >>= 1n;
  }
  return result;
}

export function md5(data: string | Uint8Array | object): string {
  const input = typeof data === 'object' && !(data instanceof Uint8Array) ? JSON.stringify(data) : data;
  return createHash('md5').update(input).digest('hex');
}

export function sha1(data: string | Uint8Array): string {
  return createHash('sha1').update(data).digest('hex');
}

export function randomString(length = 16, lowercase = false): string {
  const alphabet = lowercase
    ? '1234567890abcdefghijklmnopqrstuvwxyz'
    : '1234567890ABCDEFGHIJKLMNOPQRSTUVWXYZ';
  const bytes = randomBytes(length);
  let out = '';
  for (let i = 0; i < length; i++) out += alphabet[bytes[i] % alphabet.length];
  return out;
}

export function randomGuid(): string {
  const hex = randomBytes(16).toString('hex');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20, 32)}`;
}

/** Derive decimal device MID from a GUID string via MD5 big-endian integer conversion. */
export function calculateMid(guid: string): string {
  return BigInt('0x' + md5(guid)).toString(10);
}

/** AES-256-CBC encryption matching KuGouMusicApi's cryptoAesEncrypt (hex output). */
export function aesEncrypt(data: string | object, opt: { key: string; iv: string }): string;
export function aesEncrypt(data: string | object, opt?: { key?: string }): { str: string; key: string };
export function aesEncrypt(
  data: string | object,
  opt?: { key?: string; iv?: string },
): string | { str: string; key: string } {
  const plain = toUtf8Bytes(data);
  if (opt?.key && opt?.iv) {
    const cipher = createCipheriv(
      'aes-256-cbc',
      Buffer.from(opt.key, 'utf8'),
      Buffer.from(opt.iv, 'utf8'),
    );
    return Buffer.from(concatBytes(cipher.update(plain), cipher.final())).toString('hex');
  }
  const tempKey = opt?.key || randomString(16, true);
  const derivedKey = md5(tempKey).slice(0, 32);
  const derivedIv = derivedKey.slice(16);
  const cipher = createCipheriv(
    'aes-256-cbc',
    Buffer.from(derivedKey, 'utf8'),
    Buffer.from(derivedIv, 'utf8'),
  );
  const str = Buffer.from(concatBytes(cipher.update(plain), cipher.final())).toString('hex');
  return opt?.key ? str : { str, key: tempKey };
}

/** AES-256-CBC decryption matching KuGouMusicApi's cryptoAesDecrypt (hex input). */
export function aesDecrypt<T = unknown>(hexData: string, key: string, iv?: string): T | string {
  const derivedKey = iv ? key : md5(key).slice(0, 32);
  const derivedIv = iv || derivedKey.slice(16);
  const decipher = createDecipheriv(
    'aes-256-cbc',
    Buffer.from(derivedKey, 'utf8'),
    Buffer.from(derivedIv, 'utf8'),
  );
  const plain = Buffer.from(
    concatBytes(decipher.update(Buffer.from(hexData, 'hex')), decipher.final()),
  ).toString('utf8');
  try {
    return JSON.parse(plain) as T;
  } catch {
    return plain;
  }
}

/** Raw (zero-right-padded) RSA encryption matching KuGouMusicApi's cryptoRSAEncrypt. */
export function rsaRawEncrypt(data: string | object): string {
  const bytes = toUtf8Bytes(data);
  if (bytes.length > RSA_KEY_BYTES) throw new Error('RSA payload exceeds key length');
  const padded = new Uint8Array(RSA_KEY_BYTES);
  padded.set(bytes, 0);
  const msg = BigInt('0x' + Buffer.from(padded).toString('hex'));
  return modPow(msg, PUBLIC_LITE_RSA_E, PUBLIC_LITE_RSA_N)
    .toString(16)
    .padStart(RSA_KEY_BYTES * 2, '0');
}

/** PKCS#1 v1.5 RSA encryption matching KuGouMusicApi's rsaEncrypt2. */
export function rsaPkcs1Encrypt(data: string | object): string {
  // padding: 1 = RSA_PKCS1_PADDING in haiku-jsrt crypto.
  return publicEncrypt({ key: PUBLIC_LITE_RSA_PEM, padding: 1 }, toUtf8Bytes(data)).toString('hex');
}

/** AES-128-CBC encryption used by device registration (register_dev). */
export function deviceAesEncrypt(data: string | object): { key: string; str: string } {
  const key = randomString(6, true);
  const digest = md5(key);
  const cipher = createCipheriv(
    'aes-128-cbc',
    Buffer.from(digest.slice(0, 16), 'utf8'),
    Buffer.from(digest.slice(16, 32), 'utf8'),
  );
  const encrypted = concatBytes(cipher.update(toUtf8Bytes(data)), cipher.final());
  return { key, str: Buffer.from(encrypted).toString('base64') };
}

/** AES-128-CBC decryption for register_dev responses. */
export function deviceAesDecrypt<T = unknown>(bytes: Uint8Array, key: string): T | string {
  const digest = md5(key);
  const decipher = createDecipheriv(
    'aes-128-cbc',
    Buffer.from(digest.slice(0, 16), 'utf8'),
    Buffer.from(digest.slice(16, 32), 'utf8'),
  );
  const plain = Buffer.from(concatBytes(decipher.update(bytes), decipher.final())).toString('utf8');
  try {
    return JSON.parse(plain) as T;
  } catch {
    return plain;
  }
}

export function signatureAndroidParams(
  params: Record<string, string | number | boolean | undefined>,
  body = '',
): string {
  const joined = Object.keys(params)
    .filter((k) => params[k] !== undefined)
    .sort()
    .map((k) => `${k}=${params[k]}`)
    .join('');
  return md5(`${ANDROID_SIGN_SALT}${joined}${body}${ANDROID_SIGN_SALT}`);
}

export function signatureWebParams(
  params: Record<string, string | number | boolean | undefined>,
  body = '',
): string {
  const joined = Object.keys(params)
    .filter((k) => params[k] !== undefined)
    .map((k) => `${k}=${params[k]}`)
    .sort()
    .join('');
  return md5(`${WEB_SIGN_SALT}${joined}${body}${WEB_SIGN_SALT}`);
}

export function signKey(
  hash: string,
  mid: string,
  userid: number | string = 0,
  appid: number | string = APPID,
): string {
  return md5(`${hash}${SIGN_KEY_SALT}${appid}${mid}${userid || 0}`);
}

export function signParamsKey(
  data: string | number,
  appid: number | string = APPID,
  clientver: number | string = CLIENTVER,
): string {
  return md5(`${appid}${ANDROID_SIGN_SALT}${clientver}${data}`);
}

/** Decode base64-encoded Kugou KRC container (4-byte header + 16-byte XOR + zlib). */
export function decodeKrc(base64Content: string): string {
  const raw = Buffer.from(base64Content, 'base64');
  if (raw.length <= 4) return '';
  const body = new Uint8Array(raw.length - 4);
  for (let i = 0; i < body.length; i++) body[i] = raw[i + 4] ^ KRC_XOR_KEY[i % KRC_XOR_KEY.length];
  try {
    return Buffer.from(inflateSync(body)).toString('utf8');
  } catch {
    return '';
  }
}
