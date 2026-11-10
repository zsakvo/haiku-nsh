import { definePlugin, field, RateLimited } from 'haiku';
import type { Track, Album, Artist, Playlist, Program, Toplist, Detail, Page, Context } from 'haiku';
const song: Track = { id: 'x', title: 'Title', artists: ['Artist'] };
const album: Album = { id: 'a', title: 'Album' }, artist: Artist = { id: 'a', name: 'Artist' };
const playlist: Playlist = { id: 'p', title: 'Playlist', owned: true }, program: Program = { id: 'p', title: 'Program' }, toplist: Toplist = { id: 't', title: 'Toplist' };
const page = <T>(value: T): Page<T> => ({ items: [value], next: 'opaque' });
const detail = <T>(value: T): Detail<T> => ({ info: value, tracks: page(song) });
const login = async () => ({ credential: { token: 'fixture' }, user: { name: 'Fixture' } });
export default definePlugin({
 id: 'fixture.all', name: 'All capabilities', version: '1.0.0', http: { cookies: true, headers: { Accept: 'application/json' } },
 connection: { fields: { baseUrl: field.url('URL'), password: field.secret('Password') }, identity: ['baseUrl'], async verify(conn, ctx) { await ctx.http.post(conn.baseUrl, { json: { ping: true } }).json<unknown>(); return { name: 'Fixture' }; } },
 auth: { qrcode: { variants: { qr: 'QR' }, async start() { return { image: 'data:', state: {}, expiresIn: 60 }; }, async poll() { return { status: 'done' as const, ...await login() }; } }, sms: { async send() { return { state: {}, cooldown: 60 }; }, verify: login }, password: { fields: { password: field.secret('Password') }, submit: login }, cookie: { submit: login }, web: { url: 'https://example.invalid', done: login }, async user() { return { name: 'Fixture' }; }, async refresh() { return { token: 'fresh' }; }, async logout() {} },
 search: { async track() { return page(song); }, async album() { return page(album); }, async artist() { return page(artist); }, async playlist() { return page(playlist); }, async program() { return page(program); } },
 track: { async get() { return song; }, async streams(id, ctx) { ctx.log.info(id, ctx.locale, ctx.credential); return [{ url: 'https://example.invalid/stream', quality: ctx.quality }]; }, async lyrics() { return { format: 'lrc', text: '[00:00]Lyrics' }; } },
 album: { async get() { return detail(album); } }, artist: { async get() { return detail(artist); }, async tracks() { return page(song); }, async albums() { return page(album); } }, playlist: { async get() { return detail(playlist); } }, toplist: { async list() { return [toplist]; }, async get() { return detail(toplist); } }, program: { async get() { return detail(program); } },
 radio: { async channels() { return [{ id: 'r', title: 'Radio' }]; }, async next() { return [song]; } },
 async home() { return { shelves: [{ id: 's', title: 'Songs', type: 'tracks', items: [song] }, { id: 'e', title: 'Entries', type: 'entries', items: [{ title: 'Entry', to: { album: 'a' } }] }] }; },
 categories: { async list() { return { sorts: [{id:'hot',title:'热门'},{id:'new',title:'最新'}], groups: [{ title: 'Group', items: [{ id: 'c', title: 'Category' }] }] }; }, async playlists() { return page(playlist); } }, me: { async playlists() { return page(playlist); }, async likedTracks() { return page(song); } },
 library: { cards: [{ id: 'songs', label: 'Songs', entity: 'track' }], async browse() { return page(song); } },
 scrape: { async search() { return [{ id: 's', title: 'Title', artists: ['Artist'] }]; }, async lyrics() { return null; } },
});
// The fixture also exercises explicit context and structured error types.
const check = (ctx: Context) => { ctx.storage.set('test', { ok: true }); ctx.storage.get<{ ok: boolean }>('test'); throw new RateLimited({ retryAfter: 30 }); };
void check;
