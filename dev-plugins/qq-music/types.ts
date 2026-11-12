// Defines the minimal upstream response fields used by the QQ Music plugin.
// Keeps field aliases and shared authentication response shapes in one place.
// Ported from QQMusicApi 108617f (GPL-3.0).

// Song responses use mid/name; DoSearchForQQMusicMobile uses singerMID/singerName/singerPic.
export interface Singer {
  mid?: string;
  name?: string;
  singerMID?: string;
  singerName?: string;
  singerPic?: string;
}
// Song/detail albums use id/mid/name; search uses albumID/albummid/albumName and singer_list.
// Album detail/new-album summaries also expose albumMid, publishDate and singers.
export interface RemoteAlbum {
  id?: number;
  mid?: string;
  name?: string;
  albumID?: number;
  albumMid?: string;
  albummid?: string;
  albumName?: string;
  pic?: string;
  publishDate?: string;
  desc?: string;
  singer_list?: Singer[];
  singers?: Singer[];
}
// Song detail/search both expose title/name; title takes precedence when both are present.
export interface Song {
  id: number;
  mid: string;
  title?: string;
  name?: string;
  singer: Singer[];
  album?: RemoteAlbum;
  interval?: number;
  type?: number;
  file?: {
    media_mid?: string;
    size_128mp3?: number;
    size_320mp3?: number;
    size_flac?: number;
    size_192ogg?: number;
    size_192aac?: number;
    size_96ogg?: number;
    size_96aac?: number;
    size_48aac?: number;
    size_new?: number[];
  };
  pay?: { pay_play?: number };
}
// GetPlaylistByUin/GetPlaylistFavInfo use tid/dirName/picUrl/songNum and nick/nickname.
// Search/tag CGI use dissid/dissname/logo or imgurl/introduction; CgiGetDiss dirinfo uses
// id/title/picurl or cover/songnum/desc and creator.nick (QQMusicApi SongList aliases).
export interface RemotePlaylist {
  id?: number;
  tid?: number;
  dissid?: number;
  title?: string;
  dirName?: string;
  dissname?: string;
  name?: string;
  picurl?: string;
  picUrl?: string;
  logo?: string;
  imgurl?: string;
  cover?: string;
  songnum?: number;
  songNum?: number;
  desc?: string;
  introduction?: string;
  nickname?: string;
  nick?: string;
  creator?: { nick?: string };
  invalid?: boolean;
}
// GetAll/GetDetail share TopSummary: frontPicUrl is the cover, headPicUrl is the fallback header.
export interface RemoteTop {
  topId: number;
  title: string;
  frontPicUrl?: string;
  headPicUrl?: string;
  totalNum?: number;
}
export interface SongPage {
  songList: { songInfo: Song }[];
  totalNum: number;
}
export interface AlbumDetail {
  basicInfo: RemoteAlbum;
  singer?: { singerList: Singer[] };
}
export interface SingerDetail {
  singer_list: {
    basic_info: { name: string };
    ex_info?: { desc?: string };
    pic?: { pic?: string };
  }[];
}
export interface Tags {
  code: number;
  data: {
    categories: {
      categoryGroupName: string;
      items: { categoryId: number; categoryName: string }[];
    }[];
  };
}
export interface PlaylistDetail {
  dirinfo: RemotePlaylist;
  songlist: Song[];
  total_song_num: number;
  hasmore: number;
}
export interface Card {
  id: string | number;
  type: number | string;
  subtype?: number | string;
  jumptype?: number | string;
  title?: string;
  subtitle?: string;
  songMid?: string;
  cover?: string | { default_url?: string; medium_url?: string };
  card_extra_info?: {
    Tracks?: Song[];
    SongList?: { ID: number; MID: string; Name: string; SingerName: string; Cover: string }[];
  };
}
export interface FeedShelf {
  id: number;
  title_content: string;
  title_template?: string;
  v_niche: { v_card: Card[] }[];
}
export interface Feed {
  retcode?: number;
  v_shelf: FeedShelf[];
}

export interface Credential {
  musicid: number;
  musickey: string;
  str_musicid: string;
  login_type: number;
  openid: string;
  access_token: string;
  refresh_token: string;
  expired_at: number;
  unionid: string;
  refresh_key: string;
  encrypt_uin: string;
}
export interface Envelope<T> {
  code?: number;
  data?: T;
}
export interface LoginResult extends Partial<Credential> {
  loginType?: number;
  encryptUin?: string;
}
export interface LoginUser {
  nick?: string;
  nickname?: string;
  headurl?: string;
}
