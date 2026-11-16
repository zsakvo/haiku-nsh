import { colorScheme, nativeComponent, token, View, type JSX } from "nsh";
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
export function WindowPanel(props: { children?: JSX.Element; bleedRight?: boolean }) {
  // bleedRight: the viewport runs to the window's right edge so scroll bars sit flush against it,
  // while the rounded card is drawn as a background inset by panel-gap.
  if (props.bleedRight) return <View class="flex-1 min-w-0 min-h-0 overflow-hidden"
    style={{ marginTop: token("panel-gap"), marginBottom: token("panel-gap"), marginLeft: token("panel-gap"),
      borderTopLeftRadius: token("radius-panel"), borderBottomLeftRadius: token("radius-panel") }}>
    <View class="absolute inset-y-0 left-0 rounded-(--radius-panel) bg-background pointer-events-none"
      style={{ right: token("panel-gap") }} />
    {props.children}
  </View>;
  return <View class="flex-1 min-w-0 min-h-0 rounded-(--radius-panel) overflow-hidden bg-background"
    style={{ margin: token("panel-gap") }}>{props.children}</View>;
}
export function ShellToolbar(props: { children?: JSX.Element; testID?: string }) {
  return <View testID={props.testID} class="shrink-0 flex-row items-center justify-end gap-2 pl-(--content-inset-left) pr-(--content-inset-right)"
    style={{ height: chromeHeight() }} desktop={{ windowDrag: true }}>{props.children}</View>;
}
export function NavigationToolbar(props: { children?: JSX.Element }) {
  return <View testID="navigation-toolbar"
    class="shrink-0 flex-row items-start gap-(--navigation-button-gap) pt-(--navigation-toolbar-inset) px-(--navigation-toolbar-inset)"
    style={{ height: Math.max(chromeHeight(), token("compact-control-height") + 2 * token("navigation-toolbar-inset")),
      paddingRight: token("navigation-toolbar-inset") + token("panel-gap") }}
    desktop={{ windowDrag: true }}>{props.children}</View>;
}

/** AppKit's live within-window blur; registered only on macOS. */
const GlassSurface = nativeComponent<{ radius: number; dark: boolean }>(
  "haiku.GlassSurface", { props: ["radius", "dark"] },
);
/** macOS: navigation and source share one blurred capsule floating over the content. */
export function FloatingNavigationCapsule(props: { children?: JSX.Element }) {
  const radius = () => token("compact-control-height") / 2 + 2;
  return <View testID="navigation-toolbar"
    class="absolute flex-row items-center gap-(--navigation-button-gap) p-0.5 rounded-full border"
    style={{ top: token("navigation-toolbar-inset"), right: token("navigation-toolbar-inset") + token("panel-gap"),
      // Softer than the `border` token: a faint hairline over the blur.
      borderColor: colorScheme() === "dark" ? "rgba(255,255,255,0.08)" : "rgba(0,0,0,0.07)" }}
    desktop={{ windowDrag: true }}>
    <View class="absolute inset-0 rounded-full overflow-hidden pointer-events-none">
      <GlassSurface class="absolute inset-0 pointer-events-none" radius={radius()} dark={colorScheme() === "dark"}
        style={{ opacity: token("dock-backdrop-opacity") }} />
      {/* A white wash, not grey: outline, shadow and blur separate it from the content. */}
      <View class="absolute inset-0"
        style={{ backgroundColor: colorScheme() === "dark" ? "rgba(28,27,26,0.6)" : "rgba(255,255,255,0.72)" }} />
    </View>
    {props.children}
  </View>;
}
export function CapsuleDivider() {
  return <View class="w-px h-4 mx-1 bg-border" />;
}
