import { cleanup, colorScheme, context, effect, nativeComponent, platform, signal, token, View, type JSX } from "nsh";
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
  return <View class="flex-1 min-w-0 min-h-0 overflow-hidden bg-background">{props.children}</View>;
}
export function ShellToolbar(props: { children?: JSX.Element; testID?: string }) {
  return <View testID={props.testID} class="shrink-0 flex-row items-center justify-end gap-2 pl-(--content-inset-left) pr-(--content-inset-right)"
    style={{ height: chromeHeight() }} desktop={{ windowDrag: true }}>{props.children}</View>;
}
/** AppKit's live within-window blur; registered only on macOS. */
export const GlassSurface = nativeComponent<{ radius: number; dark: boolean; material?: "header" | "popover" }>(
  "haiku.GlassSurface", { props: ["radius", "dark", "material"] },
);
/** Shared frosted glass fill for top navigation bar controls (buttons, search, source capsule). */
export function NavigationControlBackdrop(props: { pill?: boolean }) {
  const macos = platform.os === "macos";
  const radiusClass = props.pill ? "rounded-full" : "rounded-(--radius)";
  const radius = () => props.pill ? token("navigation-control-height") / 2 : token("radius");
  return <View class={`absolute inset-0 overflow-hidden ${radiusClass} pointer-events-none`}>
    <If when={macos}>
      <GlassSurface class="absolute inset-0 pointer-events-none" radius={radius()} material="popover" dark={colorScheme() === "dark"} />
    </If>
    <View class="absolute inset-0 bg-source-chip-tint transition-colors duration-200 ease-out motion-reduce:transition-none group-hover:bg-accent group-active:bg-muted group-selected:bg-accent" />
  </View>;
}
export const NavigationLayoutContext = context<{
  inset: () => number;
  /** Whether a page's content has scrolled beneath the bar; pages report false when inactive. */
  report: (page: object, scrolled: boolean) => void;
} | undefined>(undefined);

/** A list reaches behind the toolbar while its leading space scrolls with its content. */
export function NavigationScrollArea(props: { children?: JSX.Element }) {
  const layout = NavigationLayoutContext();
  return <View class="flex-1 min-h-0" style={{ marginTop: -(layout?.inset() ?? 0) }}>{props.children}</View>;
}
export function NavigationScrollHeader(props: { children?: JSX.Element; class?: string }) {
  const layout = NavigationLayoutContext();
  return <View class={props.class} style={{ paddingTop: layout?.inset() ?? 0 }}>{props.children}</View>;
}
/** Returns the setter a page's list calls with "has my content left the top?". */
export function navigationScroll(active: () => boolean) {
  const layout = NavigationLayoutContext(), page = {}, scrolled = signal(false);
  effect(() => layout?.report(page, active() && scrolled()));
  cleanup(() => layout?.report(page, false));
  return (value: boolean) => scrolled.set(value);
}

/** The content's top bar. It carries the content's own colour, so it only reads as a bar once
 * something scrolls beneath it: on macOS that content shows through a blur, and a hairline appears. */
export function NavigationBar(props: { children?: JSX.Element; floating: boolean; scrolled: boolean }) {
  return <View testID="navigation-bar"
    class={`${props.floating ? "absolute top-0 inset-x-0" : "shrink-0"} flex-row items-center gap-(--navigation-button-gap) px-(--content-inset-left)`}
    style={{ height: token("navigation-bar-height") }}
    desktop={{ windowDrag: true }}>
    <View class="absolute inset-0 overflow-hidden pointer-events-none">
      <If when={props.floating}>
        <GlassSurface class="absolute inset-0 pointer-events-none" radius={0} material="header" dark={colorScheme() === "dark"} />
      </If>
      <View class={`absolute inset-0 ${props.floating ? "bg-navigation-backdrop-tint" : "bg-background"}`} />
      <If when={props.floating}>
        <View class="absolute inset-0 bg-background transition-opacity duration-200 motion-reduce:transition-none"
          style={{ opacity: props.scrolled ? 0 : 1 }} />
      </If>
      <View testID="navigation-bar-divider" class="absolute bottom-0 inset-x-0 h-px bg-navigation-backdrop-border transition-opacity duration-200 motion-reduce:transition-none"
        style={{ opacity: props.scrolled ? 1 : 0 }} />
    </View>
    {props.children}
  </View>;
}
