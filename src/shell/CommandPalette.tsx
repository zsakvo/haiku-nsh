import { Text, View } from "nsh";
import { Command, type CommandItem } from "nsh/base";
import { Dialog } from "../ui/Surfaces";
import { Kbd } from "../ui/Navigation";
import { Icon } from "../ui/Icon";

export function CommandPalette(props: { open: boolean; items: CommandItem[]; onClose: () => void; onSelect: (id: string) => void }) {
  return <Dialog open={props.open} class="w-128 max-w-full self-center mt-24 overflow-hidden"
    onOpenChange={(event) => { if (!event.open) props.onClose(); }}>
    <Command items={props.items} accessibilityLabel="命令面板" searchAccessibilityLabel="查找命令"
      autoFocus placeholder="前往页面或更改外观…" onCancel={props.onClose}
      onSelect={(event) => props.onSelect(event.id)}
      searchPrefix={<Icon name="search" small color="muted-foreground" />}
      searchBarClass="flex-row items-center gap-2 px-4 py-3 border-b border-border"
      searchClass="flex-1 text-base text-foreground placeholder:text-muted-foreground"
      listClass="max-h-96 p-1.5"
      itemClass="group min-h-8 rounded-(--radius) px-3 py-1 hover:bg-accent focus:bg-selection active:bg-muted text-sm text-popover-foreground"
      groupClass="px-3 pt-3 pb-1 text-xs text-muted-foreground"
      separatorClass="h-px my-2 bg-border"
      renderItem={(item) => <View class="flex-1 flex-row items-center justify-between gap-4">
        <Text class="text-sm text-popover-foreground">{item.label}</Text>
        {item.shortcutLabel && <Kbd label={item.shortcutLabel} />}
      </View>}
      empty={<Text class="p-6 text-sm text-muted-foreground">没有匹配的命令</Text>}
      footer={<View class="flex-row items-center gap-2 px-4 py-2 border-t border-border">
        <Kbd label="↑ ↓" /><Text class="text-xs text-muted-foreground">选择</Text>
        <Kbd label="↵" /><Text class="text-xs text-muted-foreground">前往</Text>
        <View class="flex-1" /><Kbd label="Esc" /><Text class="text-xs text-muted-foreground">关闭</Text>
      </View>} />
  </Dialog>;
}
