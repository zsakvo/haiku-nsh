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
export function ChromeSpacer() {
  return <View class="shrink-0" style={{ height: chromeHeight() }} desktop={{ windowDrag: true }} />;
}
export function WindowPanel(props: { children?: JSX.Element }) {
  return <View class="flex-1 min-w-0 min-h-0 rounded-(--radius-panel) overflow-hidden bg-background"
    style={{ margin: token("panel-gap") }}>{props.children}</View>;
}
export function ShellToolbar(props: { children?: JSX.Element }) {
  return <View class="shrink-0 flex-row items-center gap-2 px-(--content-inset) border-b border-border"
    style={{ height: chromeHeight() }} desktop={{ windowDrag: true }}>{props.children}</View>;
}
