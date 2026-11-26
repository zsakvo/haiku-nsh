import { signal, Text, View } from "nsh";
import type { Core } from "haiku/native";
import { Button } from "../ui/Button";
import { Field } from "../ui/LibraryControls";
import { Dialog } from "../ui/Surfaces";
import type { Collection } from "../state/collections";

/** Shared create/edit dialog; create may atomically follow with the selected tracks. */
export function PlaylistEditor(props: {
  core: Core; collection?: Collection; tracks?: unknown[];
  onClose: () => void; onSaved?: (id: string) => void;
}) {
  const title = signal(props.collection?.title ?? "");
  const description = signal(props.collection?.description ?? "");
  const error = signal("");
  const busy = signal(false);
  async function save() {
    error.set(""); busy.set(true);
    try {
      const service = props.core.collections;
      const id = props.collection?.id ?? service.create(title());
      if (props.collection || description()) service.rename(id, title(), description());
      if (props.tracks?.length) service.add(id, props.tracks);
      props.onSaved?.(id); props.onClose();
    } catch (e) { error.set(String(e)); } finally { busy.set(false); }
  }
  return <Dialog open onOpenChange={(event) => { if (!event.open) props.onClose(); }}
    class="w-112 p-5 gap-4">
    <Text role="header" class="text-lg font-semibold text-foreground">
      {props.collection ? "编辑歌单" : "新建歌单"}
    </Text>
    <Field label="歌单名称" value={title()} onChange={(value) => title.set(value)} />
    <Field label="歌单描述" value={description()} onChange={(value) => description.set(value)} />
    <If when={!!error()}><Text class="text-sm text-destructive">{error()}</Text></If>
    <View class="flex-row justify-end gap-2">
      <Button label="取消" onPress={props.onClose} />
      <Button label="保存歌单" variant="primary" disabled={busy() || !title().trim()}
        onPress={() => void save()} />
    </View>
  </Dialog>;
}
