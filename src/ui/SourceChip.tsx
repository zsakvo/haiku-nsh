import { Image, platform, signal, Text, token, View, type ViewRef } from "nsh";
import { Button as BaseButton, Select } from "nsh/base";
import { isAvailable as desktopAvailable, Menu as DesktopMenu } from "nsh/desktop";
import type { SourceOption } from "../shell/pageSource";
import { focus } from "./Button";
import { NavigationControlBackdrop } from "./Chrome";
import { popupItem, popupSurface } from "./Surfaces";

function SourceMark(props: { logo?: string; testID?: string }) {
  return <Image testID={props.testID} src={props.logo || "asset:/ui/icons/antenna.svg"}
    class="size-(--source-icon-size) shrink-0 rounded-sm" style={props.logo ? {} : {tintColor:token("source-chip-foreground")}} />;
}

/** A glass capsule source selector sharing the navigation toolbar's control height. */
export function SourceChip(props: {
  items: SourceOption[]; value: string; disabled?: boolean; onChange: (value: string) => void;
}) {
  const selected = () => props.items.find((item) => item.id === props.value) ?? props.items[0];
  const nativeMenu = platform.os === "macos" && desktopAvailable;
  const triggerClass = `group relative h-(--source-chip-height) max-w-56 flex-row items-center gap-1.5 overflow-hidden rounded-full border border-source-chip-accent-border pl-1.5 pr-2.5 disabled:opacity-35 ${focus}`;
  const triggerBody = () => <>
    <NavigationControlBackdrop pill />
    <SourceMark testID="source-chip-icon" logo={selected()?.logo} />
    <Text testID="source-chip-label" class="text-source-chip-foreground" style={{numberOfLines:1,fontSize:token("source-label-size")}}>{selected()?.title ?? "选择来源"}</Text>
  </>;

  if (nativeMenu) {
    const open = signal(false);
    let anchor: ViewRef | undefined;
    async function openMenu() {
      if (props.disabled || open() || !props.items.length) return;
      open.set(true);
      try {
        const measured = await anchor?.measure();
        const at = measured
          ? { x: measured.pagePosition.x, y: measured.pagePosition.y + measured.bounds.size.height + 4 }
          : undefined;
        await DesktopMenu.popup(
          props.items.map((item) => ({
            label: item.title,
            icon: item.logo || "asset:/ui/icons/antenna.svg",
            checked: item.id === props.value,
            onSelect: () => {
              if (item.id !== props.value) props.onChange(item.id);
            },
          })),
          at,
        );
      } finally {
        open.set(false);
      }
    }
    return <View ref={(node) => { anchor = node; }} class="shrink-0 max-w-56">
      <BaseButton testID="source-chip" accessibilityLabel="来源" disabled={props.disabled}
        selected={open()} onPress={() => void openMenu()} class={triggerClass}>
        {triggerBody()}
      </BaseButton>
    </View>;
  }

  return <Select testID="source-chip" accessibilityLabel="来源" value={props.value}
    items={props.items.map((item) => ({ value: item.id, title: item.title }))}
    disabled={props.disabled} align="end" class="shrink-0 max-w-56"
    popupClass={`${popupSurface} min-w-40 p-1`} listClass="max-h-64" itemClass={`${popupItem} selected:bg-accent`}
    onChange={(event) => { if (event.value !== null) props.onChange(event.value); }}
    trigger={<View class={triggerClass}>{triggerBody()}</View>}
    renderItem={(entry) => <View class="flex-row items-center gap-2">
      <SourceMark logo={props.items.find((item) => item.id === entry.value)?.logo} />
      <Text class="flex-1 text-sm text-popover-foreground">{entry.title}</Text>
    </View>} />;
}
