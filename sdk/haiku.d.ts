declare module 'crypto' {
  /** Binary data returned by crypto operations. */
  export interface Buffer extends Uint8Array {
    toString(encoding?: string): string;
  }

  /** Create a digest builder for the selected hash algorithm. */
  export function createHash(algorithm: string): {
    update(data: string | Uint8Array): ReturnType<typeof createHash>;
    digest(encoding: 'hex' | 'base64'): string;
  };

  /** Generate cryptographically secure random bytes. */
  export function randomBytes(length: number): Buffer;

  /** Create an encryptor with an explicit key and initialization vector. */
  export function createCipheriv(
    algorithm: string,
    key: Uint8Array,
    iv: Uint8Array | null,
  ): Cipher;

  /** Create a decryptor with an explicit key and initialization vector. */
  export function createDecipheriv(
    algorithm: string,
    key: Uint8Array,
    iv: Uint8Array | null,
  ): Cipher;

  /** Incremental cipher operations and authenticated encryption controls. */
  export interface Cipher {
    update(data: string | Uint8Array, inputEncoding?: string): Buffer;
    final(): Buffer;
    setAutoPadding(value?: boolean): this;
    getAuthTag(): Buffer;
    setAuthTag(value: Uint8Array): this;
    setAAD(value: Uint8Array): this;
  }

  /** Encrypt bytes with a PEM public key and optional RSA parameters. */
  export function publicEncrypt(
    key: string | {
      key: string;
      padding?: number;
      oaepHash?: string;
      oaepLabel?: Uint8Array;
    },
    data: Uint8Array,
  ): Buffer;
}

declare module 'haiku' {
  /** Values that can cross the plugin host boundary. */
  export type Json = null | boolean | number | string | Json[] | {
    [key: string]: Json | undefined;
  };

  /** Opaque pagination token supplied by a plugin. */
  export type Cursor = string;

  /** Image URL, optionally containing a {size} placeholder. */
  export type Image = string;

  /** Icon identifier used by a library card. */
  export type Icon = string;

  /** Requested or available audio quality. */
  export type Quality = 'standard' | 'high' | 'lossless' | 'hires';

  /** Supported audio container formats. */
  export type Format = 'mp3' | 'flac' | 'm4a' | 'ogg' | 'opus';

  /** Pagination parameters passed to a capability handler. */
  export interface PageArgs {
    cursor?: Cursor;
    limit: number;
  }

  /** One page of entities with an optional continuation token. */
  export interface Page<T> {
    items: T[];
    next?: Cursor;
    total?: number;
  }

  /** An artist name with optional native identity and artwork. */
  export interface Artist {
    id?: string;
    name: string;
    cover?: Image;
    description?: string;
  }

  /** Album metadata using the upstream native identifier. */
  export interface Album {
    id: string;
    title: string;
    artists?: Artist[] | string[];
    cover?: Image;
    year?: number;
    trackCount?: number;
    description?: string;
    extra?: Json;
  }

  /** Track metadata using the upstream native identifier. */
  export interface Track {
    id: string;
    title: string;
    artists: Artist[] | string[];
    album?: {
      id?: string;
      title: string;
    };
    cover?: Image;
    duration?: number;
    quality?: Quality[];
    explicit?: boolean;
    extra?: Json;
  }

  /** Playlist metadata, including optional ownership information. */
  export interface Playlist {
    id: string;
    title: string;
    cover?: Image;
    creator?: string;
    trackCount?: number;
    description?: string;
    owned?: boolean;
  }

  /** Audio program metadata and optional episode count. */
  export interface Program {
    id: string;
    title: string;
    cover?: Image;
    host?: string;
    episodeCount?: number;
    description?: string;
  }

  /** Chart metadata shown in discovery lists. */
  export interface Toplist {
    id: string;
    title: string;
    subtitle?: string;
    cover?: Image;
  }

  /** Playable URL with optional headers, format and expiry metadata. */
  export interface Stream {
    url: string;
    headers?: Record<string, string>;
    format?: Format;
    quality?: Quality;
    bitrate?: number;
    size?: number;
    expiresAt?: number;
  }

  /** Lyrics content and its serialization format. */
  export interface Lyrics {
    format: 'lrc' | 'krc' | 'yrc' | 'ttml' | 'plain';
    text: string;
    translation?: string;
  }

  /** Persistent JSON storage scoped to one source instance. */
  export interface KV {
    get<T extends Json = Json>(key: string): T | null;
    set(key: string, value: Json): void;
    delete(key: string): void;
    clear(): void;
  }

  /** Structured logging with host-side credential redaction. */
  export interface Logger {
    debug(...args: unknown[]): void;
    log(...args: unknown[]): void;
    info(...args: unknown[]): void;
    warn(...args: unknown[]): void;
    error(...args: unknown[]): void;
    trace(...args: unknown[]): void;
  }

  /** Request parameters for the context HTTP client. */
  export interface HttpOptions {
    query?: Record<string, string | number | boolean | undefined>;
    headers?: Record<string, string>;
    timeout?: number;
    throwOnError?: boolean;
    json?: Json;
    form?: Record<string, string>;
    body?: string | Uint8Array;
  }

  /** Lazy HTTP request with response decoding helpers. */
  export interface HttpResult {
    json<T>(): Promise<T>;
    text(): Promise<string>;
    bytes(): Promise<Uint8Array>;
    response(): Promise<Response>;
  }

  /** Source-scoped HTTP client with host defaults and cancellation. */
  export interface Http {
    get(url: string, options?: HttpOptions): HttpResult;
    post(url: string, options?: HttpOptions): HttpResult;
  }

  /** Host services and credentials supplied to every capability handler. */
  export interface Context {
    signal: AbortSignal;
    http: Http;
    storage: KV;
    credential: Json;
    connection: Record<string, string> | null;
    locale: string;
    log: Logger;
    crypto: typeof import('crypto');
  }

  /** Declarative connection or login field metadata. */
  export interface Field {
    type: string;
    label: string;
    required?: boolean;
    placeholder?: string;
    value?: Json;
    options?: string[];
  }

  /** Field definitions indexed by their submitted value keys. */
  export type Fields = Record<string, Field>;

  /** Builders for supported connection and login field types. */
  export const field: Record<
    'text' | 'url' | 'secret' | 'number' | 'boolean' | 'select' | 'textarea',
    (label: string, options?: Omit<Partial<Field>, 'type' | 'label'>) => Field
  >;

  /** Authenticated user information for source presentation. */
  export interface User {
    id?: string;
    name: string;
    avatar?: string;
    vip?: string;
  }

  /** Successful login result persisted by the host. */
  export interface Login {
    credential: Json;
    user?: User;
  }

  /** Cookie captured from an isolated web login session. */
  export interface Cookie {
    name: string;
    value: string;
    domain?: string;
    path?: string;
    secure?: boolean;
    httpOnly?: boolean;
    expires?: number;
  }

  /** Authentication handlers and optional credential lifecycle hooks. */
  export interface Auth {
    qrcode?: {
      variants?: Record<string, string>;
      start(variant: string, ctx: Context): Promise<{
        image?: string;
        url?: string;
        state: Json;
        expiresIn?: number;
      }>;
      poll(state: Json, ctx: Context): Promise<
        { status: 'waiting' | 'scanned' | 'expired' } | ({ status: 'done' } & Login)
      >;
    };
    sms?: {
      send(phone: string, ctx: Context): Promise<{
        state: Json;
        cooldown?: number;
      }>;
      verify(state: Json, code: string, ctx: Context): Promise<Login>;
    };
    password?: {
      fields?: Fields;
      submit(values: Record<string, string>, ctx: Context): Promise<Login>;
    };
    cookie?: {
      submit(cookie: string, ctx: Context): Promise<Login>;
    };
    web?: {
      url: string;
      userAgent?: string;
      done(
        page: { url: string; cookies: Cookie[] },
        ctx: Context,
      ): Login | null | Promise<Login | null>;
    };
    user(ctx: Context): Promise<User | null>;
    refresh?(ctx: Context): Promise<Json>;
    logout?(ctx: Context): Promise<void>;
  }

  /** Library connection fields, identity keys and verification handler. */
  export interface Connection {
    fields: Fields;
    identity: string[];
    verify(conn: Record<string, string>, ctx: Context): Promise<{
      user?: User;
      name?: string;
    }>;
  }

  /** Entity kinds supported by the search capability. */
  export type SearchKind = 'track' | 'album' | 'artist' | 'playlist' | 'program';

  /** Search entity type associated with each search kind. */
  export type EntityMap = {
    track: Track;
    album: Album;
    artist: Artist;
    playlist: Playlist;
    program: Program;
  };

  /** Optional search handlers indexed by entity kind. */
  export type Search = {
    [K in SearchKind]?: (
      args: PageArgs & { keyword: string },
      ctx: Context,
    ) => Promise<Page<EntityMap[K]>>;
  };

  /** Entity detail metadata and a page of associated tracks. */
  export interface Detail<T> {
    info?: T;
    tracks: Page<Track>;
  }

  /** Native destination identifier interpreted by the host. */
  export type Link =
    | { playlist: string }
    | { album: string }
    | { toplist: string }
    | { radio: string }
    | { category: string };

  /** Discovery entry that links to another destination. */
  export interface Entry {
    title: string;
    subtitle?: string;
    cover?: Image;
    to: Link;
  }

  /** Typed discovery shelf with optional navigation to more items. */
  export type Shelf = {
    id: string;
    title: string;
    more?: Link;
  } & (
    | { type: 'tracks'; items: Track[] }
    | { type: 'playlists'; items: Playlist[] }
    | { type: 'albums'; items: Album[] }
    | { type: 'toplists'; items: Toplist[] }
    | { type: 'programs'; items: Program[] }
    | { type: 'entries'; items: Entry[] }
  );

  /** Metadata candidate returned by a scraper. */
  export interface ScrapeCandidate {
    id: string;
    title: string;
    artists: string[];
    album?: string;
    year?: number;
    cover?: Image;
    duration?: number;
    track?: number;
    extra?: Json;
  }

  /** Plugin metadata, authentication and optional content capabilities. */
  export interface Definition {
    id: string;
    name: string;
    version: string;
    author?: string;
    logo?: string;
    description?: string;
    http?: {
      headers?: Record<string, string>;
      timeout?: number;
      cookies?: boolean;
    };
    connection?: Connection;
    auth?: Auth;
    search?: Search;
    track?: {
      get?(id: string, ctx: Context): Promise<Track>;
      streams(
        id: string,
        ctx: Context & { quality: Quality; format?: Format },
      ): Promise<Stream[]>;
      lyrics?(id: string, ctx: Context): Promise<Lyrics | null>;
    };
    album?: {
      get(id: string, ctx: Context & PageArgs): Promise<Detail<Album>>;
    };
    artist?: {
      get(id: string, ctx: Context & PageArgs): Promise<Detail<Artist>>;
      tracks?(id: string, ctx: Context & PageArgs): Promise<Page<Track>>;
      albums?(id: string, ctx: Context & PageArgs): Promise<Page<Album>>;
    };
    playlist?: {
      get(id: string, ctx: Context & PageArgs): Promise<Detail<Playlist>>;
    };
    toplist?: {
      list?(ctx: Context): Promise<Toplist[]>;
      get(id: string, ctx: Context & PageArgs): Promise<Detail<Toplist>>;
    };
    program?: {
      get(id: string, ctx: Context & PageArgs): Promise<Detail<Program>>;
    };
    radio?: {
      channels?(ctx: Context): Promise<{ id: string; title: string; cover?: Image }[]>;
      next(channelId: string | undefined, ctx: Context & { history: Track[]; count: number }): Promise<Track[]>;
    };
    home?(ctx: Context & { cursor?: Cursor; refresh: boolean }): Promise<{
      shelves: Shelf[];
      next?: Cursor;
    }>;
    categories?: {
      list(ctx: Context): Promise<{
        sorts?: { id: string; title: string }[];
        groups: {
          title: string;
          items: { id: string; title: string; cover?: Image }[];
        }[];
      }>;
      playlists(
        id: string,
        ctx: Context & PageArgs & { sort?: string },
      ): Promise<Page<Playlist>>;
    };
    me?: {
      playlists?(ctx: Context & PageArgs): Promise<Page<Playlist>>;
      likedTracks?(ctx: Context & PageArgs): Promise<Page<Track>>;
    };
    library?: {
      cards: {
        id: string;
        label: string;
        icon?: Icon;
        entity: 'track' | 'album' | 'artist' | 'playlist';
      }[];
      browse(
        id: string,
        ctx: Context & PageArgs & { sort?: string },
      ): Promise<Page<Track | Album | Artist | Playlist>>;
    };
    scrape?: {
      search(
        query: {
          title: string;
          artists: string[];
          album?: string;
          duration?: number;
        },
        ctx: Context,
      ): Promise<ScrapeCandidate[]>;
      lyrics?(candidate: ScrapeCandidate, ctx: Context): Promise<Lyrics | null>;
    };
  }

  /** Register and return a typed plugin definition. */
  export function definePlugin<T extends Definition>(definition: T): T;

  /** Structured error details preserved across the host boundary. */
  export interface ErrorFields {
    message?: string;
    retryAfter?: number;
    status?: number;
    [key: string]: Json | undefined;
  }

  /** Signal that the source requires an initial login. */
  export class AuthRequired extends Error {
    constructor(input?: string | ErrorFields);
  }

  /** Signal that credentials need refresh or a new login. */
  export class AuthExpired extends Error {
    constructor(input?: string | ErrorFields);
  }

  /** Signal that the requested content requires a subscription. */
  export class VipRequired extends Error {
    constructor(input?: string | ErrorFields);
  }

  /** Signal that content is unavailable in the current region. */
  export class GeoRestricted extends Error {
    constructor(input?: string | ErrorFields);
  }

  /** Signal that the requested upstream entity does not exist. */
  export class NotFound extends Error {
    constructor(input?: string | ErrorFields);
  }

  /** Signal throttling with an optional retry delay in seconds. */
  export class RateLimited extends Error {
    retryAfter?: number;
    constructor(input?: string | ErrorFields);
  }

  /** Report an upstream service or HTTP failure. */
  export class Upstream extends Error {
    constructor(input?: string | ErrorFields);
  }

  /** Signal that the host should request fresh playback streams. */
  export class Expired extends Error {
    constructor(input?: string | ErrorFields);
  }

  /** Report invalid or unexpected upstream response data. */
  export class ParseError extends Error {
    constructor(input?: string | ErrorFields);
  }

  /** Signal that the requested capability is unavailable. */
  export class Unsupported extends Error {
    constructor(input?: string | ErrorFields);
  }
}

declare module 'buffer' {
  /** Construct a binary buffer from encoded text or bytes. */
  export const Buffer: {
    from(data: string | Uint8Array, encoding?: string): import('crypto').Buffer;
  };
}

declare module 'zlib' {
  /** Inflate a zlib-compressed byte sequence. */
  export function inflateSync(data: Uint8Array): Uint8Array;
}
