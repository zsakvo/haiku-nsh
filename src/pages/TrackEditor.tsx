import { Scroll, signal, Text, View } from "nsh";
import { Core, LibraryService } from "haiku/native";
import type { TrackSummary } from "../state/core";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Surfaces";
import { Field } from "../ui/LibraryControls";

const fields = [
  ["title", "标题"], ["artists", "艺术家（用 / 分隔）"], ["album", "专辑"],
  ["album_artist", "专辑艺术家"], ["year", "年份"], ["track_no", "曲号"], ["disc_no", "碟号"],
] as const;
type FieldKey = typeof fields[number][0] | "cover";
/** An editor draft is local. Only fields changed during this session enter the sparse patch. */
export function TrackEditor(props: { core: Core; ids: string[]; onClose: () => void }) {
  const first = props.core.library.track(props.ids[0]) as TrackSummary;
  const initial: Record<FieldKey, string> = {
    title: first.title, artists: first.artists.map((a) => typeof a === "string" ? a : a.name).join(" / "),
    album: first.album?.title ?? "", album_artist: String(first.extra?.album_artist ?? ""),
    year: String(first.extra?.year ?? ""), track_no: String(first.extra?.track_no ?? ""),
    disc_no: String(first.extra?.disc_no ?? ""), cover: first.cover ?? "",
  };
  const draft = signal({ ...initial });
  const error = signal("");
  const busy = signal(false);
  const confirm = signal<"reset" | "write" | "discard" | null>(null);
  const changed = () => Object.keys(initial).some((key) => draft()[key as FieldKey] !== initial[key as FieldKey]);
  function close() { if (changed()) confirm.set("discard"); else props.onClose(); }
  async function act(action: () => Promise<unknown>) {
    busy.set(true); error.set("");
    try { await action(); } catch (e) { error.set(String(e)); } finally { busy.set(false); }
  }
  async function save() {
    const patch: Record<string, unknown> = {};
    for (const key of Object.keys(initial) as FieldKey[]) {
      if (draft()[key] === initial[key]) continue;
      const raw = draft()[key].trim();
      if (!raw) throw new Error("字段不能留空；请用重置恢复文件标签。");
      if (["year", "track_no", "disc_no"].includes(key)) {
        if (!/^\d+$/.test(raw) || Number(raw) > 4294967295) throw new Error("年份、曲号和碟号须为非负整数。");
        patch[key] = Number(raw);
      } else if (key === "artists") {
        patch[key] = raw.split("/").map((v) => v.trim()).filter(Boolean);
      } else if (key === "cover") {
        patch[key] = await props.core.library.importCover(raw);
      } else patch[key] = raw;
    }
    await props.core.library.edit(props.ids, patch);
  }
  return <Dialog open
    closeOnBackdropPress={false} closeOnEscape={false} onCancel={close}
    class="w-128 max-h-full p-5 gap-4">
    <Text role="header" class="text-lg font-semibold text-foreground">
      {props.ids.length > 1 ? `批量编辑 ${props.ids.length} 首歌曲` : "编辑歌曲信息"}
    </Text>
    <Text class="text-xs text-muted-foreground">{props.ids.length > 1
      ? "以第一首信息为初始值；仅修改过的字段会应用到所有选中歌曲。"
      : "修改保存为应用内覆盖；重新扫描会保留这些修改。"}</Text>
    <Scroll class="min-h-0 max-h-100" contentClass="gap-3">
      {fields.map(([key, label]) => <Field label={label} value={draft()[key]}
        onChange={(value) => draft.update((d) => ({ ...d, [key]: value }))} />)}
      <View class="gap-1">
        <Text class="text-xs text-muted-foreground">{draft().cover ? "已选择封面" : "暂无封面"}</Text>
        <Button label="导入封面…" size="sm" onPress={() => void act(async () => {
          const path = await LibraryService.pickCover();
          if (path) draft.update((d) => ({ ...d, cover: path }));
        })} />
      </View>
    </Scroll>
    <If when={!!error()}><Text class="text-sm text-destructive">{error()}</Text></If>
    <If when={!!confirm()}>
      <View class="gap-2 border-t border-border pt-3">
        <Text class="text-sm text-foreground">{confirm() === "write"
          ? "写回将修改所选音频文件，应用内重置无法撤销文件内容。"
          : confirm() === "reset" ? "重置所选歌曲的全部应用内覆盖，恢复当前扫描的文件标签？" : "放弃未保存的修改？"}</Text>
        <View class="flex-row gap-2">
          <Button label="取消" disabled={busy()} onPress={() => confirm.set(null)} />
          <Button label={confirm() === "write" ? "确认写回" : confirm() === "reset" ? "确认重置" : "放弃修改"}
            disabled={busy()} onPress={() => void act(async () => {
              if (confirm() === "discard") { props.onClose(); return; }
              if (confirm() === "reset") { await props.core.library.reset(props.ids); props.onClose(); return; }
              if (changed()) await save();
              const errors = await props.core.library.writeTags(props.ids);
              if (errors.length) { error.set(errors.join("\n")); confirm.set(null); } else props.onClose();
            })} />
        </View>
      </View>
    </If>
    <If when={!confirm()}>
      <View class="flex-row gap-2 flex-wrap">
        <Button label="重置…" size="sm" disabled={busy()} onPress={() => confirm.set("reset")} />
        <Button label="写回文件标签…" size="sm" disabled={busy()} onPress={() => confirm.set("write")} />
        <View class="flex-1" />
        <Button label="取消" disabled={busy()} onPress={close} />
        <Button label="保存" variant="primary" disabled={busy() || !changed()} onPress={() => void act(async () => {
          await save(); props.onClose();
        })} />
      </View>
    </If>
  </Dialog>;
}
