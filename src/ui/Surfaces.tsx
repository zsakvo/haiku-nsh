import { effect, keymap, signal, Text, View, type JSX } from "nsh";
import { Dialog as BaseDialog, Menu as BaseMenu, ContextMenu as BaseContextMenu,
  MenuItem as BaseMenuItem, Tooltip as BaseTooltip, Sheet } from "nsh/base";
import type { DialogProps, MenuProps, ContextMenuProps, MenuItemProps } from "nsh/base";
export const popupSurface = "rounded-(--radius-surface) border border-border bg-popover shadow-lg";
export const popupItem = "rounded-(--radius) min-h-8 px-3 py-1.5 text-sm text-popover-foreground hover:bg-accent active:bg-muted focus:bg-accent checked:bg-accent disabled:opacity-40";
export function Tooltip(props: { label: string }) {
  return <BaseTooltip class={`${popupSurface} px-2 py-1`}>
    <Text class="text-xs text-popover-foreground">{props.label}</Text>
  </BaseTooltip>;
}
export function Dialog(props: DialogProps) {
  return <BaseDialog {...props} animation="none" class={`${popupSurface} ${props.class ?? ""}`}
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
      popupStyle={{ top: 0, bottom: 112, left: 0, right: 0 }}
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
