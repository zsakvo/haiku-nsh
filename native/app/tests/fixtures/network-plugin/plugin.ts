let downloadRecovered = false;
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
let homeOffline=false, homeFailOnce=false, radioBatch=0;
let searches: string[] = [];
let playlistOffline = false;
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
  async home(ctx) {
    if(homeOffline||homeFailOnce){homeFailOnce=false;throw new Upstream("推荐来源暂时离线");}
    return {shelves:[
      {id:"entries",title:"发现入口",type:"entries",items:ctx.refresh?[]:[{title:"华语分类",to:{category:"cn"}},{title:"林间频道",to:{radio:"forest"}}]},
      {id:"tracks",title:"今日推荐",type:"tracks",items:[track("1",ctx),track("2",ctx)]},
      {id:"playlists",title:"精选歌单",type:"playlists",items:[playlist(ctx),{...playlist(ctx),id:"playlist/晨",title:"晨间轻听"}],more:{category:"cn"}},
      {id:"albums",title:"新专辑",type:"albums",items:[album(ctx)]},
      {id:"toplists",title:"排行榜",type:"toplists",items:[{id:"chart",title:"回声榜",cover:album(ctx).cover}]},
      {id:"programs",title:"声音节目",type:"programs",items:[{id:"program/1",title:"林间来信",cover:album(ctx).cover}]},
      {id:"empty",title:"空货架",type:"tracks",items:[]},
    ]};
  },
  categories:{
    async list(){return {groups:[{title:"语种",items:[{id:"cn",title:"华语"},{id:"en",title:"欧美"},{id:"jp",title:"日语"}]},{title:"心情",items:[{id:"quiet",title:"安静"},{id:"happy",title:"愉悦"}]},{title:"场景",items:[{id:"night",title:"夜晚"},{id:"travel",title:"旅行"}]}],sorts:[{id:"hot",title:"热门"},{id:"new",title:"最新"}]};},
    async playlists(id,ctx){if(ctx.cursor&&ctx.cursor!=="category:第二页")throw new Error("bad cursor");return {items:Array.from({length:ctx.cursor?2:4},(_,i)=>({...playlist(ctx),id:`playlist/category/${i+(ctx.cursor?4:0)}`,title:`${ctx.sort==="new"?"最新":"热门"}精选 ${i+1+(ctx.cursor?4:0)}`})),next:ctx.cursor?undefined:"category:第二页"};},
  },
  radio:{
    async channels(){return [{id:"forest",title:"林间频道"},{id:"night",title:"深夜频道"},{id:"jazz",title:"爵士漫步"}];},
    async next(channelId,ctx){
      if(channelId!==undefined&&!channelId)throw new Error("empty radio id");
      const start=radioBatch++*4;
      return [...(ctx.history.length?[ctx.history[0]]:[]),...Array.from({length:4},(_,i)=>({...track(`radio/${start+i}`,ctx),title:`${channelId===undefined?"私人 FM":"林间频道"} · ${start+i+1}`}))];
    },
  },
  program:{async get(id,ctx){return {info:{id,title:"林间来信"},tracks:tracks(ctx)};}},
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
      if (id === "download-retry" && !downloadRecovered) throw new Upstream("下载夹具暂时离线");
      if (id === "download-slow") return [{ url: `${ctx.connection!.url}/download-slow`, format: "flac", quality: ctx.quality }];
      ctx.log.info(`streams ${id} quality=${ctx.quality}`);
      return [{url:`${ctx.connection!.url}/song.flac`,format:"flac",quality:ctx.quality,expiresAt:Math.floor(Date.now()/1000)+3600}];
    },
    async get(id, ctx) { if (id === "download-recover") downloadRecovered = true; if(id === "playlist-offline")playlistOffline=true; if(id === "home-offline")homeOffline=true; if(id === "home-online")homeOffline=false; if(id === "home-fail")homeFailOnce=true; if(id === "search-counts")return {...track("1",ctx),title:JSON.stringify(searches)}; if (id === "fail") failAlbum = true; return track("1",ctx); },
  },
  album: { async get(id, ctx) { guard(); return {info:album(ctx),tracks:tracks(ctx)}; } },
  artist: {
    async get(id, ctx) { return {info:artist(ctx),tracks:{items:[]}}; },
    async tracks(id, ctx) { return tracks(ctx); },
    async albums(id, ctx) { return {items:[album(ctx)]}; },
  },
  playlist: {
    async get(id, ctx) {
      if (playlistOffline) throw new Upstream("歌单来源离线");
      return { info: playlist(ctx), tracks: tracks(ctx) };
    },
  },
  scrape: {
    async search(query) {
      if (query.title === "Slow") await new Promise(() => {});
      return [{
        id: query.title,
        title: query.title === "Elsewhere" ? "Unrelated" : query.title,
        artists: query.title === "Song Alpha" ? query.artists : ["Other"],
        duration: query.duration,
        album: "刮削专辑",
        year: 2026,
      }];
    },
  },
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
