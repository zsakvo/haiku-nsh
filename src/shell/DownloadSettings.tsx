import { cleanup, effect, Scroll, signal, Text, View } from "nsh";
import { LibraryService } from "haiku/native";
import type { createSettings } from "../state/settings";
import { Button } from "../ui/Button";
import { Field } from "../ui/LibraryControls";
import { SettingSelect, SettingSwitch } from "../ui/SettingsControls";
import { Dialog } from "../ui/Surfaces";

interface DirectoryProgress {
  running: boolean; total: number; completed: number; moved: number; errors: string[];
}

/** New jobs use durable download preferences; existing jobs retain their destination. */
export function DownloadSettings(props: { settings: ReturnType<typeof createSettings>; initialDirectory?: string }) {
  const value = () => props.settings.value()!;
  const directory = signal(props.initialDirectory ?? value().downloadDir), naming = signal(value().downloadNaming);
  const error = signal("");
  const confirming = signal(!!props.initialDirectory), busy = signal(false);
  const progress = signal<DirectoryProgress>({ running: false, total: 0, completed: 0, moved: 0, errors: [] as string[] });
  const reported = signal(false);
  effect(() => {
    const core = props.settings.core();
    if (!core) return;
    progress.set(core.downloads.directorySnapshot() as DirectoryProgress);
    cleanup(core.downloads.onDirectoryChange(next => progress.set(next as DirectoryProgress)));
  });
  async function changeDirectory(mode: "move" | "keep") {
    const core = props.settings.core();
    if (!core) return;
    busy.set(true); error.set(""); reported.set(false);
    try {
      progress.set(await core.downloads.changeDirectory(directory(), mode) as DirectoryProgress);
      directory.set(value().downloadDir);
      confirming.set(false); reported.set(true);
    } catch (e) { error.set(String(e)); } finally { busy.set(false); }
  }
  const modes = [
    { value: "none", title: "不保存" }, { value: "embed", title: "嵌入音频" },
    { value: "sidecar", title: "独立文件" }, { value: "both", title: "嵌入与独立文件" },
  ];
  async function pickDirectory() {
    try {
      const folders = await LibraryService.pickFolders();
      if (folders[0]) {
        directory.set(folders[0]);
        confirming.set(true);
      }
      error.set("");
    } catch (e) { error.set(String(e)); }
  }
  return <View testID="download-settings" class="flex-col gap-4">
    <Text role="header" class="text-lg font-semibold text-foreground">下载</Text>
    <Text class="text-sm text-muted-foreground">新任务使用下列设置。更换目录时，可选择移动已有文件或保留原位置。</Text>
    <Field label="下载目录" value={directory()} onChange={v => directory.set(v)} placeholder="~/Music/Haiku" />
    <View class="flex-row gap-2">
      <Button label="选择下载目录…" disabled={busy()} onPress={() => void pickDirectory()} />
      <Button label="保存目录" disabled={busy() || directory() === value().downloadDir}
        onPress={() => confirming.set(true)} />
    </View>
    <Text class="text-xs text-muted-foreground">留空使用 ~/Music/Haiku。</Text>
    <SettingSelect label="同时下载" value={String(value().downloadConcurrency)}
      items={[1, 2, 3, 4, 5].map(n => ({ value: String(n), title: `${n} 个任务` }))}
      onChange={v => void props.settings.update({ downloadConcurrency: Number(v) })} />
    <SettingSelect label="下载音质" value={value().downloadQuality}
      items={[["standard", "标准"], ["high", "高品质"], ["lossless", "无损"], ["hires", "高解析"]]
        .map(([value, title]) => ({ value, title }))}
      onChange={downloadQuality => void props.settings.update({ downloadQuality })} />
    <Field label="文件名模板" value={naming()} onChange={v => naming.set(v)} />
    <View class="flex-row gap-3 items-center">
      <Text class="flex-1 text-xs text-muted-foreground">{"可用：{artist}、{title}、{album}、{index}"}</Text>
      <Button label="保存模板" onPress={() => void props.settings.update({ downloadNaming: naming() })} />
    </View>
    <SettingSelect label="目录布局" value={value().downloadLayout}
      items={[["flat", "平铺"], ["artist", "艺术家"], ["artist-album", "艺术家 / 专辑"],
        ["playlist", "歌单"], ["provider", "来源"]].map(([value, title]) => ({ value, title }))}
      onChange={downloadLayout => void props.settings.update({ downloadLayout })} />
    <SettingSelect label="封面保存" value={value().downloadCoverMode} items={modes}
      onChange={downloadCoverMode => void props.settings.update({ downloadCoverMode })} />
    <SettingSwitch label="优先高清封面" checked={value().downloadCoverHd}
      onChange={downloadCoverHd => void props.settings.update({ downloadCoverHd })} />
    <SettingSelect label="歌词保存" value={value().downloadLyricsMode} items={modes}
      onChange={downloadLyricsMode => void props.settings.update({ downloadLyricsMode })} />
    <SettingSwitch label="歌词包含译文" checked={value().downloadLyricsTranslation}
      onChange={downloadLyricsTranslation => void props.settings.update({ downloadLyricsTranslation })} />
    <SettingSwitch label="复用同音质完整媒体缓存" checked={value().downloadReuseMediaCache}
      onChange={downloadReuseMediaCache => void props.settings.update({ downloadReuseMediaCache })} />
    <If when={!!error()}><Text class="text-sm text-destructive">{error()}</Text></If>
    <If when={busy() || progress().running}>
      <Text class="text-sm text-muted-foreground">
        {`正在迁移 ${progress().completed} / ${progress().total} · 已移动 ${progress().moved} 个文件`}
      </Text>
    </If>
    <If when={reported()}>
      <Text class="text-sm text-foreground">{`目录已更新 · 移动 ${progress().moved} 个文件 · ${progress().errors.length} 项失败`}</Text>
    </If>
    <If when={!!progress().errors.length}>
      <Scroll class="max-h-40" contentClass="flex-col gap-1">
        <Each of={progress().errors} key={message => message}>{message => <Text class="text-xs text-destructive">{message()}</Text>}</Each>
      </Scroll>
    </If>
    <Dialog open={confirming()} onOpenChange={e => {
      if (!e.open && !busy()) { confirming.set(false); directory.set(value().downloadDir); }
    }} class="w-128 max-w-full self-center mt-24 p-5 gap-4">
      <Text role="header" class="text-lg font-semibold text-foreground">更换下载目录？</Text>
      <Text class="text-sm text-muted-foreground">{directory() || "~/Music/Haiku"}</Text>
      <Text class="text-sm text-muted-foreground">移动会暂停调度并等待下载释放文件，完成后恢复。失败文件保留原路径；保留只更改新任务的默认目录。</Text>
      <If when={busy()}><Text class="text-sm text-foreground">
        {`正在迁移 ${progress().completed} / ${progress().total}`}
      </Text></If>
      <If when={!!error()}><Text class="text-sm text-destructive">{error()}</Text></If>
      <View class="flex-row justify-end gap-2">
        <Button label="取消更换" disabled={busy()} onPress={() => {
          directory.set(value().downloadDir); confirming.set(false);
        }} />
        <Button label="保留原文件" disabled={busy()} onPress={() => void changeDirectory("keep")} />
        <Button label="移动已有文件" variant="primary" loading={busy()} onPress={() => void changeDirectory("move")} />
      </View>
    </Dialog>
  </View>;
}
