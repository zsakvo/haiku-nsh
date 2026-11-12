import { Image, Text, token, View } from "nsh";
import { Select } from "nsh/base";
import type { SourceOption } from "../shell/pageSource";
import { focus } from "./Button";
import { popupItem, popupSurface } from "./Surfaces";

function SourceMark(props: { logo?: string; testID?: string }) {
  return <Image testID={props.testID} src={props.logo || "asset:/ui/icons/antenna.svg"}
    class="size-3.5 shrink-0 rounded-sm" style={props.logo ? {} : {tintColor:token("foreground")}} />;
}

/** A compact source selector sharing the navigation toolbar's control height. */
export function SourceChip(props: {
  items: SourceOption[]; value: string; disabled?: boolean; onChange: (value: string) => void;
}) {
  const selected = () => props.items.find((item) => item.id === props.value) ?? props.items[0];
  return <Select testID="source-chip" accessibilityLabel="来源" value={props.value}
    items={props.items.map((item) => ({ value: item.id, title: item.title }))}
    disabled={props.disabled} align="end" class="shrink-0 max-w-56"
    popupClass={`${popupSurface} min-w-40 p-1`} listClass="max-h-64" itemClass={`${popupItem} selected:bg-accent`}
    onChange={(event) => { if (event.value !== null) props.onChange(event.value); }}
    trigger={<View class={`min-h-7 max-w-56 flex-row items-center gap-1.5 rounded-full border border-border bg-muted px-2.5 py-1 hover:bg-accent active:bg-muted disabled:opacity-35 ${focus}`}>
      <SourceMark testID="source-chip-icon" logo={selected()?.logo} />
      <Text testID="source-chip-label" class="text-xs text-foreground" style={{numberOfLines:1}}>{selected()?.title ?? "选择来源"}</Text>
    </View>}
    renderItem={(entry) => <View class="flex-row items-center gap-2">
      <SourceMark logo={props.items.find((item) => item.id === entry.value)?.logo} />
      <Text class="flex-1 text-sm text-popover-foreground">{entry.title}</Text>
    </View>} />;
}
