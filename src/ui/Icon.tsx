import { Image, token } from "nsh";
const home = "asset:/ui/icons/home-simple.svg";
const grid = "asset:/ui/icons/view-grid.svg";
const radio = "asset:/ui/icons/antenna.svg";
const search = "asset:/ui/icons/search.svg";
const music = "asset:/ui/icons/music-double-note.svg";
const heart = "asset:/ui/icons/heart.svg";
const clock = "asset:/ui/icons/clock.svg";
const stats = "asset:/ui/icons/stats-up-square.svg";
const download = "asset:/ui/icons/download.svg";
const settings = "asset:/ui/icons/settings.svg";
const left = "asset:/ui/icons/nav-arrow-left.svg";
const right = "asset:/ui/icons/nav-arrow-right.svg";
const close = "asset:/ui/icons/xmark.svg";
const check = "asset:/ui/icons/check.svg";
const palette = "asset:/ui/icons/palette.svg";
const user = "asset:/ui/icons/user.svg";
const play = "asset:/ui/icons/play.svg";
const captions = "asset:/ui/icons/closed-captions-tag.svg";
const folder = "asset:/ui/icons/folder.svg";
const plug = "asset:/ui/icons/plug-type-a.svg";
const sync = "asset:/ui/icons/refresh-double.svg";
const keyboard = "asset:/ui/icons/key-command.svg";
const code = "asset:/ui/icons/code-brackets.svg";
const icons = { home, grid, radio, search, music, heart, clock, stats, download, settings, left, right, close, check, palette,
  user, play, captions, folder, plug, sync, keyboard, code };
export type IconName = keyof typeof icons;
export function Icon(props: {
  name: IconName;
  color?: "foreground" | "primary" | "muted-foreground" | "sidebar-foreground" | "cover-foreground";
  small?: boolean;
  sidebar?: boolean;
  navigation?: boolean;
}) {
  // The refresh glyph fills its SVG; chevrons occupy only half the canvas height.
  return <Image src={icons[props.name]} class={props.navigation
    ? props.name === "sync" ? "size-(--navigation-refresh-icon-size)" : "size-(--navigation-icon-size)"
    : props.sidebar ? "size-(--sidebar-icon-size)" : props.small ? "size-4" : "size-5"}
    style={{ tintColor: token(props.color ?? "foreground") }} />;
}
