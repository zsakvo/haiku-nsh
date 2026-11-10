import { definePlugin, field, Upstream } from "haiku";
import type { Context, PageArgs, Track, Album, Artist, Playlist } from "haiku";
const track = (id: string, ctx: Context): Track => ({
  id, title: id === "1" ? "潮汐之间" : "远方回声",
  artists: [{ id: "artist/海", name: "林间回声" }],
  album: { id: "album/潮汐", title: "潮汐与回声" },
  cover: `${ctx.connection!.url}/cover.png`, duration: 60,
});
const album = (ctx: Context): Album => ({ id: "album/潮汐", title: "潮汐与回声", artists: ["林间回声"],
  cover: `${ctx.connection!.url}/cover.png`, trackCount: 2, year: 2026, description: "在潮汐与林间，收集缓慢而明亮的声音。" });
const artist = (ctx: Context): Artist => ({ id: "artist/海", name: "林间回声", cover: `${ctx.connection!.url}/cover.png`,
  description: "以自然为灵感的独立音乐人。" });
const playlist = (ctx: Context): Playlist => ({ id: "playlist/夜", title: "夜航精选", creator: "回声编辑部",
  cover: `${ctx.connection!.url}/cover.png`, description: "留给安静夜晚的音乐。" });
function tracks(ctx: Context & PageArgs) {
  return ctx.cursor ? { items: [track("2", ctx)], total: 2 }
    : { items: [track("1", ctx)], next: "opaque:page/二", total: 2 };
}
let failAlbum = false;
let searches: string[] = [];
async function searching(keyword: string, ctx: Context) {
  searches.push(keyword);
  if(keyword === "慢请求") { ctx.log.info("slow search started"); await new Promise(r=>setTimeout(r,900)); }
  if(keyword === "来源错误")throw new Upstream("搜索来源暂时离线");
}

function guard() {
  if (failAlbum) {
    failAlbum = false;
    throw new Upstream("夹具连接暂时失败，请重试");
  }
}
export default definePlugin({
  id: "network.fixture", name: "回声音乐库", version: "1.0.0",
  connection: { fields: { url: field.url("服务器") }, identity: ["url"], async verify() { return {}; } },
  search: {
    async track({keyword,cursor},ctx) {
      await searching(keyword,ctx);if(keyword === "空结果")return {items:[]};
      return {items:Array.from({length:cursor?2:5},(_,i)=>({...track(String(i+1+(cursor?5:0)),ctx),title:`${keyword} · ${i+1+(cursor?5:0)}`})),next:cursor?undefined:"search:第二页"};
    },
    async album({keyword,cursor},ctx) {await searching(keyword,ctx);return {items:[{...album(ctx),title:`${keyword} · 专辑`}],next:cursor?undefined:"album:下一页"};},
    async artist({keyword},ctx) {await searching(keyword,ctx);return {items:[{...artist(ctx),name:`${keyword} · 歌手`}]};},
    async playlist({keyword},ctx) {await searching(keyword,ctx);return {items:Array.from({length:4},(_,i)=>({...playlist(ctx),id:`playlist/${i}`,title:`${keyword} · 歌单 ${i+1}`}))};},
    async program({keyword},ctx) {await searching(keyword,ctx);return {items:[{id:"program/1",title:`${keyword} · 节目`}]};},
  },
  track: {
    async streams(id, ctx) {
      ctx.log.info(`streams ${id} quality=${ctx.quality}`);
      return [{url:`${ctx.connection!.url}/song.flac`,format:"flac",quality:ctx.quality,expiresAt:Math.floor(Date.now()/1000)+3600}];
    },
    async get(id, ctx) { if(id === "search-counts")return {...track("1",ctx),title:JSON.stringify(searches)}; if (id === "fail") failAlbum = true; return track("1",ctx); },
  },
  album: { async get(id, ctx) { guard(); return {info:album(ctx),tracks:tracks(ctx)}; } },
  artist: {
    async get(id, ctx) { return {info:artist(ctx),tracks:{items:[]}}; },
    async tracks(id, ctx) { return tracks(ctx); },
    async albums(id, ctx) { return {items:[album(ctx)]}; },
  },
  playlist: { async get(id, ctx) { return {info:playlist(ctx),tracks:tracks(ctx)}; } },
  toplist: { async get(id, ctx) { return {info:{id,title:"回声榜",cover:album(ctx).cover},tracks:tracks(ctx)}; } },
  library: {
    cards: [
      {id:"albums",label:"专辑",entity:"album"}, {id:"artists",label:"歌手",entity:"artist"},
      {id:"playlists",label:"歌单",entity:"playlist"}, {id:"tracks",label:"歌曲",entity:"track"},
    ],
    async browse(id, ctx) {
      return {items:id==="albums"?[album(ctx)]:id==="artists"?[artist(ctx)]:id==="playlists"?[playlist(ctx)]:[track("1",ctx),track("2",ctx)]};
    },
  },
});
