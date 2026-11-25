// Minimal upstream response and session interfaces for the Kugou Music (Lite) plugin.
// Field shapes follow MakcRe/KuGouMusicApi#5d6c9f7 (--platform=lite) and MoeKoeMusic.

export interface Credential {
  userid: number;
  token: string;
  t1: string;
  vip_type: number;
  vip_token: string;
  nickname: string;
  pic: string;
}

export interface DeviceState {
  guid: string;
  mid: string;
  dev: string;
  mac: string;
  dfid: string;
}

export interface Envelope<T = unknown> {
  status?: number;
  errcode?: number;
  error_code?: number;
  errmsg?: string;
  error?: string;
  data?: T;
}

export interface LoginData {
  userid?: number;
  token?: string;
  t1?: string;
  vip_type?: number;
  vip_token?: string;
  nickname?: string;
  pic?: string;
  secu_params?: string;
  info_list?: { userid: number; nickname?: string; pic?: string; p_grade?: number }[];
}

export interface UserDetailData {
  userid?: number;
  nickname?: string;
  pic?: string;
  descri?: string;
  p_grade?: number;
}

export interface VipDetailData {
  busi_vip?: {
    is_vip?: number;
    product_type?: string;
    vip_end_time?: string;
  }[];
}

export interface RemoteSinger {
  id?: number | string;
  author_id?: number | string;
  AuthorId?: number | string;
  name?: string;
  author_name?: string;
  AuthorName?: string;
  avatar?: string;
  sizable_avatar?: string;
  Avatar?: string;
  intro?: string;
  song_count?: number;
  album_count?: number;
}

export interface SearchSong {
  FileHash?: string;
  HQFileHash?: string;
  SQFileHash?: string;
  ResFileHash?: string;
  SongName?: string;
  OriSongName?: string;
  FileName?: string;
  SingerName?: string;
  Singers?: { id?: number; name?: string }[];
  AlbumID?: string | number;
  AlbumName?: string;
  Duration?: number;
  Image?: string;
  MixSongID?: number | string;
  PublishDate?: string;
  PublishTime?: string;
}

export interface SearchAlbum {
  albumid?: number | string;
  album_id?: number | string;
  albumname?: string;
  album_name?: string;
  img?: string;
  imgurl?: string;
  sizable_cover?: string;
  singername?: string;
  author_name?: string;
  singers?: { id?: number | string; name?: string; author_id?: number | string; author_name?: string }[];
  authors?: { author_id?: number | string; author_name?: string }[];
  publish_time?: string;
  publishtime?: string;
  publish_date?: string;
  songcount?: number;
  song_count?: number;
  intro?: string;
}

export interface SearchPlaylist {
  gid?: string;
  global_collection_id?: string;
  specialid?: number | string;
  listid?: number | string;
  list_create_gid?: string;
  list_create_userid?: number;
  list_create_username?: string;
  specialname?: string;
  name?: string;
  img?: string;
  flexible_cover?: string;
  pic?: string;
  nickname?: string;
  song_count?: number;
  count?: number;
  intro?: string;
  sort?: number;
  authors?: unknown[];
}

export interface PrivilegeItem {
  hash?: string;
  quality?: string;
  level?: number;
  privilege?: number;
  status?: number;
  pay_type?: number;
  fail_process?: number;
  album_id?: string | number;
  album_audio_id?: string | number;
  name?: string;
  songname?: string;
  singername?: string;
  albumname?: string;
  info?: {
    duration?: number;
    filesize?: number;
    bitrate?: number;
    extname?: string;
    image?: string;
  };
  trans_param?: {
    union_cover?: string;
    ogg_128_hash?: string;
    ogg_128_filesize?: number;
    ogg_320_hash?: string;
    ogg_320_filesize?: number;
  };
  relate_goods?: PrivilegeItem[];
}

export interface SongUrlResponse {
  status?: number;
  errcode?: number;
  error_code?: number;
  url?: string[];
  backupUrl?: string[];
  extName?: string;
  bitRate?: number;
  fileSize?: number;
  timeLength?: number;
  priv_status?: number;
}

export interface AlbumSong {
  audio_info?: {
    hash?: string;
    hash_128?: string;
    hash_320?: string;
    hash_flac?: string;
    hash_high?: string;
    duration?: number;
  };
  base?: {
    audio_name?: string;
    author_name?: string;
    album_audio_id?: number | string;
  };
  authors?: RemoteSinger[];
  album_info?: {
    album_id?: number | string;
    album_name?: string;
  };
  trans_param?: {
    union_cover?: string;
  };
}

export interface ArtistSong {
  hash?: string;
  hash_320?: string;
  hash_flac?: string;
  hash_high?: string;
  audio_name?: string;
  author_name?: string;
  authors?: RemoteSinger[];
  album_id?: number | string;
  album_name?: string;
  album_audio_id?: number | string;
  timelength?: number;
  trans_param?: { union_cover?: string };
}

export interface PlaylistTrack {
  hash?: string;
  name?: string;
  singerinfo?: { id?: number; name?: string }[];
  albuminfo?: { id?: number | string; name?: string };
  mixsongid?: number | string;
  cover?: string;
  timelen?: number;
  relate_goods?: { level?: number; quality?: string }[];
}

export interface RankItem {
  rankid?: number | string;
  rankname?: string;
  imgurl?: string;
  bannerurl?: string;
  intro?: string;
  update_frequency?: string;
}

export interface RankSong {
  songname?: string;
  author_name?: string;
  authors?: RemoteSinger[];
  album_id?: number | string;
  album_name?: string;
  mixsongid?: number | string;
  deprecated?: {
    hash?: string;
    hash_320?: string;
    hash_flac?: string;
    hash_high?: string;
    duration?: number;
  };
  trans_param?: { union_cover?: string };
}

export interface RecommendSong {
  hash?: string;
  songname?: string;
  ori_audio_name?: string;
  audio_name?: string;
  name?: string;
  filename?: string;
  author_name?: string;
  singername?: string;
  authors?: RemoteSinger[];
  singerinfo?: { id?: number | string; name?: string }[];
  album_id?: number | string;
  ori_album_id?: number | string;
  album_name?: string;
  album_audio_id?: number | string;
  mixsongid?: number | string;
  sizable_cover?: string;
  album_sizable_cover?: string;
  album_cover?: string;
  cover?: string;
  pic?: string;
  time_length?: number | string;
  timelength?: number | string;
  timelength_128?: number | string;
  duration?: number | string;
  hash_128?: string;
  hash_320?: string;
  hash_flac?: string;
  hash_high?: string;
  album_info?: {
    album_id?: number | string;
    album_name?: string;
    sizable_cover?: string;
    cover?: string;
  };
  audio_info?: {
    hash?: string;
    hash_128?: string;
    hash_320?: string;
    hash_flac?: string;
    hash_high?: string;
    timelength?: number | string;
    timelength_128?: number | string;
    duration?: number | string;
    trans_param?: { union_cover?: string };
  };
  trans_param?: { union_cover?: string };
}

export interface YouthCardInfo {
  card_id?: number;
  card_title?: string;
  label_name?: string;
  label_desc?: string;
  cover_pic?: string;
  song_name?: string;
}

export interface YouthCardData {
  card_info?: YouthCardInfo;
  song_list?: RecommendSong[];
  tag_list?: {
    label_id?: number | string;
    label_name?: string;
    label_desc?: string;
    cover_pic?: string;
    top_cover_pic?: string;
    song_list?: RecommendSong[];
  }[];
}

export interface YouthChannel {
  global_collection_id?: string;
  channel_id?: number | string;
  name?: string;
  channel_name?: string;
  title?: string;
  cover?: string;
  pic?: string;
  img?: string;
  sizable_cover?: string;
  intro?: string;
  desc?: string;
  song_count?: number;
  count?: number;
  nickname?: string;
  create_user_name?: string;
}

export interface TopIpItem {
  id?: number | string;
  type?: number;
  title?: string;
  sub_title?: string;
  image_url?: string;
  extra?: {
    global_collection_id?: string;
    global_special_id?: string;
    specialid?: number | string;
  };
}

export interface HomeDiscoverItem {
  item_id?: string;
  item_type?: string;
  song_info?: RecommendSong;
}

export interface PlaylistTagGroup {
  tag_id?: number | string;
  tag_name?: string;
  son?: { tag_id?: number | string; tag_name?: string; img?: string }[];
}

export interface LyricCandidate {
  id: string;
  accesskey: string;
  score?: number;
}
