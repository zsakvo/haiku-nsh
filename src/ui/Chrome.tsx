import { token, View, type JSX } from "nsh";
import { titleBar } from "nsh/desktop";

/** One geometry policy for the main window and the settings surface. */
export const chromeHeight = () => Math.max(token("toolbar-height"), titleBar().height);
export function ShellFrame(props: { children?: JSX.Element }) {
  return <View class="flex-1 flex-row bg-sidebar">{props.children}</View>;
}
export function ShellSidebar(props: { children?: JSX.Element }) {
  return <View class="shrink-0 bg-sidebar" style={{ width: token("sidebar-width") }}>{props.children}</View>;
}
export function SidebarChrome(props: { children?: JSX.Element }) {
  return <View class="shrink-0 flex-row items-center justify-between pr-2 mb-(--sidebar-top-gap)"
    style={{ height: chromeHeight() }} desktop={{ windowDrag: true }}>
    <View class="shrink-0" style={{ width: titleBar().left }} />
    <View class="flex-row items-center">{props.children}</View>
  </View>;
}
export function SidebarSearch(props: { children?: JSX.Element }) {
  return <View class="shrink-0 px-2 pb-(--sidebar-search-gap)">{props.children}</View>;
}
export function WindowPanel(props: { children?: JSX.Element }) {
  return <View class="flex-1 min-w-0 min-h-0 rounded-(--radius-panel) overflow-hidden bg-background"
    style={{ margin: token("panel-gap") }}>{props.children}</View>;
}
export function ShellToolbar(props: { children?: JSX.Element; testID?: string }) {
  return <View testID={props.testID} class="shrink-0 flex-row items-center justify-end gap-2 px-(--content-inset)"
    style={{ height: chromeHeight() }} desktop={{ windowDrag: true }}>{props.children}</View>;
}
export function NavigationToolbar(props: { children?: JSX.Element }) {
  return <View testID="navigation-toolbar"
    class="shrink-0 flex-row items-start gap-(--navigation-button-gap) pt-(--navigation-toolbar-inset) px-(--navigation-toolbar-inset)"
    style={{ height: Math.max(chromeHeight(), token("compact-control-height") + 2 * token("navigation-toolbar-inset")) }}
    desktop={{ windowDrag: true }}>{props.children}</View>;
}
