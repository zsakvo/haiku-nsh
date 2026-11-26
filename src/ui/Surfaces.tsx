import { effect, keymap, platform, signal, Text, View, type JSX, type ViewRef } from "nsh";
import { Button as BaseButton, Dialog as BaseDialog, Menu as BaseMenu, ContextMenu as BaseContextMenu,
  MenuItem as BaseMenuItem, Tooltip as BaseTooltip, Sheet } from "nsh/base";
import type { DialogProps, MenuProps, ContextMenuProps, MenuItemProps } from "nsh/base";
import { isAvailable as desktopAvailable, Menu as DesktopMenu } from "nsh/desktop";
import { focus } from "./Button";
import { Icon } from "./Icon";
export const popupSurface = "rounded-(--radius-surface) border border-border bg-popover shadow-lg";
export const popupItem = "rounded-(--radius-inset) min-h-6 px-2.5 py-1 text-sm text-popover-foreground active:bg-muted focus:bg-accent checked:bg-accent disabled:opacity-40";
export function Tooltip(props: { label: string }) {
  return <BaseTooltip class={`${popupSurface} px-2 py-1`}>
    <Text class="text-xs text-popover-foreground">{props.label}</Text>
  </BaseTooltip>;
}
/** Dialogs center in the window; `placement="top"` anchors a Spotlight-style palette near the top. */
export function Dialog(props: DialogProps & { placement?: "center" | "top" }) {
  return <BaseDialog {...props} animation="none"
    class={`${props.placement === "top" ? "mx-auto mt-20" : "m-auto"} max-h-full max-w-full ${popupSurface} ${props.class ?? ""}`}
    backdropClass="bg-overlay-backdrop" />;
}
/** Modal state is independent of the retained page tree and navigation. */
export function Overlay(props: { open: boolean; onClose: () => void; children?: JSX.Element }) {
  const visible = signal(props.open);
  // The settings shortcut must also work while its search input has focus.
  // Register only while this surface is open; Command owns its own Escape.
  effect(() => {
    if (props.open) {
      visible.set(true);
      return keymap.bind("escape", () => props.onClose(), { when: "always" });
    }
  });
  return <>
    <If when={visible()}><View class="absolute inset-0 z-10 bg-sidebar" /></If>
    <Sheet open={props.open} side="bottom" popupClass="flex-col bg-sidebar"
      popupStyle={{ top: 0, bottom: 0, left: 0, right: 0 }}
      backdropClass="pointer-events-none" closeOnBackdropPress={false}
      onClose={() => visible.set(false)}
      onOpenChange={(event) => { if (!event.open) props.onClose(); }}>
      {props.children}
    </Sheet>
  </>;
}
export function Menu(props: MenuProps) {
  return <BaseMenu {...props} popupClass={`${popupSurface} min-w-48 p-1`} />;
}
export function ContextMenu(props: ContextMenuProps) {
  return <BaseContextMenu {...props} popupClass={`${popupSurface} min-w-48 p-1`} />;
}
export function MenuItem(props: MenuItemProps) {
  return <BaseMenuItem {...props} class={popupItem} />;
}
/** Whether menus should open as the system NSMenu instead of a drawn popover. */
export const nativeMenus = platform.os === "macos" && desktopAvailable;
/** Opens a native menu below an anchor view; `open()` drives the trigger's selected state. */
export function nativePopup() {
  const open = signal(false);
  let anchor: ViewRef | undefined;
  return {
    open,
    ref: (node: ViewRef) => { anchor = node; },
    async show(items: Parameters<typeof DesktopMenu.popup>[0]) {
      if (open()) return;
      open.set(true);
      try {
        const measured = await anchor?.measure();
        const at = measured
          ? { x: measured.pagePosition.x, y: measured.pagePosition.y + measured.bounds.size.height + 4 }
          : undefined;
        await DesktopMenu.popup(items, at);
      } finally {
        open.set(false);
      }
    },
  };
}
export type ActionMenuItem = {
  label: string;
  disabled?: boolean;
  separatorBefore?: boolean;
  onSelect: () => void;
};
/** Hover-free overflow menu button: native NSMenu on macOS, custom Menu on other platforms. */
export function ActionMenu(props: {
  label: string;
  accessibilityLabel?: string;
  testID?: string;
  compact?: boolean;
  items: ActionMenuItem[];
}) {
  const nativeMenu = platform.os === "macos" && desktopAvailable;
  const triggerClass = props.compact
    ? `size-5 items-center justify-center rounded-full active:bg-muted selected:bg-muted ${focus}`
    : `size-8 items-center justify-center rounded-(--radius) active:opacity-60 selected:bg-muted ${focus}`;
  const triggerContent = () => props.label === "…"
    ? <Icon name="harmonyosDotGrid1x2" class={props.compact ? "size-3.5" : "size-4"} color={props.compact ? "muted-foreground" : "foreground"} />
    : <Text class="text-sm text-foreground">{props.label}</Text>;
  if (nativeMenu) {
    const open = signal(false);
    let anchor: ViewRef | undefined;
    async function openMenu() {
      if (open()) return;
      open.set(true);
      try {
        const measured = await anchor?.measure();
        const at = measured
          ? { x: measured.pagePosition.x, y: measured.pagePosition.y + measured.bounds.size.height + 4 }
          : undefined;
        await DesktopMenu.popup(
          props.items.flatMap((item) => [
            ...(item.separatorBefore ? [{ type: "separator" as const }] : []),
            {
              label: item.label,
              disabled: item.disabled,
              onSelect: item.onSelect,
            },
          ]),
          at,
        );
      } finally {
        open.set(false);
      }
    }
    return <View ref={(node) => { anchor = node; }} class="shrink-0">
      <BaseButton
        testID={props.testID}
        accessibilityLabel={props.accessibilityLabel ?? props.label}
        selected={open()}
        onPress={() => void openMenu()}
        class={triggerClass}
      >
        {triggerContent()}
      </BaseButton>
    </View>;
  }
  return <Menu trigger={
    <BaseButton
      testID={props.testID}
      accessibilityLabel={props.accessibilityLabel ?? props.label}
      onPress={() => {}}
      class={triggerClass}
    >
      {triggerContent()}
    </BaseButton>
  }>
    <Each of={props.items} key={(item) => item.label}>
      {(item) => <MenuItem disabled={item().disabled} onSelect={item().onSelect}>
        <Text>{item().label}</Text>
      </MenuItem>}
    </Each>
  </Menu>;
}
