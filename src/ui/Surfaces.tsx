import { effect, keymap, Text, type JSX } from "nsh";
import { Dialog as BaseDialog, Menu as BaseMenu, ContextMenu as BaseContextMenu,
  MenuItem as BaseMenuItem, Tooltip as BaseTooltip } from "nsh/base";
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
  // The settings shortcut must also work while its search input has focus.
  // Register only while this surface is open; Command owns its own Escape.
  effect(() => {
    if (props.open) return keymap.bind("escape", () => props.onClose(), { when: "always" });
  });
  return <BaseDialog open={props.open} animation="none" class="flex-1 bg-background"
    closeOnBackdropPress={false} onOpenChange={(event) => { if (!event.open) props.onClose(); }}>
    {props.children}
  </BaseDialog>;
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
