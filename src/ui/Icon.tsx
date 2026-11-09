import { Image, token } from "nsh";
import home from "../../assets/icons/home-simple.svg";
import grid from "../../assets/icons/view-grid.svg";
import radio from "../../assets/icons/antenna.svg";
import search from "../../assets/icons/search.svg";
import music from "../../assets/icons/music-double-note.svg";
import heart from "../../assets/icons/heart.svg";
import clock from "../../assets/icons/clock.svg";
import stats from "../../assets/icons/stats-up-square.svg";
import download from "../../assets/icons/download.svg";
import settings from "../../assets/icons/settings.svg";
import left from "../../assets/icons/nav-arrow-left.svg";
import right from "../../assets/icons/nav-arrow-right.svg";
import close from "../../assets/icons/xmark.svg";
import check from "../../assets/icons/check.svg";
import palette from "../../assets/icons/palette.svg";
import user from "../../assets/icons/user.svg";
import play from "../../assets/icons/play.svg";
import captions from "../../assets/icons/closed-captions-tag.svg";
import folder from "../../assets/icons/folder.svg";
import plug from "../../assets/icons/plug-type-a.svg";
import sync from "../../assets/icons/refresh-double.svg";
import keyboard from "../../assets/icons/key-command.svg";
import code from "../../assets/icons/code-brackets.svg";
const icons = { home, grid, radio, search, music, heart, clock, stats, download, settings, left, right, close, check, palette,
  user, play, captions, folder, plug, sync, keyboard, code };
export type IconName = keyof typeof icons;
export function Icon(props: {
  name: IconName;
  color?: "foreground" | "primary" | "muted-foreground" | "sidebar-foreground";
  small?: boolean;
}) {
  return <Image src={icons[props.name]} class={props.small ? "size-4" : "size-5"}
    style={{ tintColor: token(props.color ?? "foreground") }} />;
}
