import { signal, Text, View } from "nsh";
import { LibraryService } from "haiku/native";
import type { createSettings } from "../state/settings";
import { Button } from "../ui/Button";
import { Field } from "../ui/LibraryControls";
import { SettingSelect, SettingSwitch } from "../ui/SettingsControls";

/** New jobs use durable download preferences; existing jobs retain their destination. */
export function DownloadSettings(props: { settings: ReturnType<typeof createSettings> }) {
  const value = () => props.settings.value()!;
  const directory = signal(value().downloadDir), naming = signal(value().downloadNaming);
  const error = signal("");
  const modes = [
    { value: "none", title: "不保存" }, { value: "embed", title: "嵌入音频" },
    { value: "sidecar", title: "独立文件" }, { value: "both", title: "嵌入与独立文件" },
  ];
  async function pickDirectory() {
    try {
      const folders = await LibraryService.pickFolders();
      if (folders[0]) {
        directory.set(folders[0]);
        await props.settings.update({ downloadDir: folders[0] });
      }
      error.set("");
    } catch (e) { error.set(String(e)); }
  }
  return <View testID="download-settings" class="flex-col gap-4">
    <Text role="header" class="text-lg font-semibold text-foreground">下载</Text>
    <Text class="text-sm text-muted-foreground">设置应用于新任务，现有下载文件保留在原目录。</Text>
    <Field label="下载目录" value={directory()} onChange={v => directory.set(v)} placeholder="~/Music/Haiku" />
    <View class="flex-row gap-2">
      <Button label="选择下载目录…" onPress={() => void pickDirectory()} />
      <Button label="保存目录" onPress={() => void props.settings.update({ downloadDir: directory() })} />
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
  </View>;
}
