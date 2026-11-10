import type { Context, Json, Login, User } from 'haiku';
/** Upstream response fields vary by CGI; the v2 boundary validates their entity shape. */
export type Data = Record<string, any>;
/** Per-call protocol facade; credentials never enter plugin KV storage. */
export function createProtocol(ctx: Context): {
  search(args: Data): Promise<Data>;
  getTrack(args: Data): Promise<Data>;
  getPlayback(args: Data, context?: Data): Promise<Data>;
  getLyrics(args: Data): Promise<Data>;
  getPlaylist(args: Data): Promise<Data>;
  getHomeFeed(args: Data): Promise<Data>;
  getDiscoverCategories(): Promise<Data>;
  getRecommendedPlaylists(args: Data): Promise<Data>;
  getUserPlaylists(args: Data): Promise<Data>;
  startQr(variant: string): Promise<{ image: string; state: Json; expiresIn: number }>;
  pollQr(state: Json): Promise<any>;
  sendSms(phone: string): Promise<{ state: Json; cooldown: number }>;
  verifySms(state: Json, code: string): Promise<Login>;
  cookie(value: string): Login;
  user(): User | null;
  refresh(): Promise<Json>;
  logout(): Promise<Data>;
  request(args: Data): Promise<Data>;
};
