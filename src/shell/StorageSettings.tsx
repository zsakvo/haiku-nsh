import { resource, signal, Text, View } from "nsh";
import { SettingsService } from "haiku/native";
import type { createSettings } from "../state/settings";
import { SettingSelect, SettingSwitch } from "../ui/SettingsControls";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Surfaces";
export function StorageSettings(props: { settings: ReturnType<typeof createSettings>; onLibrary: () => void }) {
  const confirming = signal(false), busy = signal(false), error = signal("");
  const usage = resource(async () => await SettingsService.cacheUsage() as { usedBytes: number; entryCount: number });
  async function clear() {
    busy.set(true);
    try { await SettingsService.clearCache(); confirming.set(false); usage.refetch(); error.set(""); }
    catch (e) { error.set(`无法清理缓存：${String(e)}`); } finally { busy.set(false); }
  }
  return <View class="gap-4" testID="storage-settings">
    <Text role="header" class="text-lg font-semibold text-foreground">存储</Text>
    <SettingSwitch label="媒体缓存" checked={props.settings.value()!.mediaCacheEnabled} onChange={(mediaCacheEnabled) => void props.settings.update({ mediaCacheEnabled })} />
    <SettingSelect label="缓存容量" value={String(props.settings.value()!.mediaCacheMaxBytes)}
      items={[0.5,1,2,5,10,20].map((n) => ({ value: String(n * 1024 ** 3), title: n === 0.5 ? "512 MiB" : `${n} GiB` }))}
      onChange={(value) => void props.settings.update({ mediaCacheMaxBytes: Number(value) })} />
    <Text class="text-sm text-muted-foreground">{usage() ? `已用 ${(usage()!.usedBytes / 1024 ** 2).toFixed(1)} MiB · ${usage()!.entryCount} 个完整条目` : "正在读取缓存用量…"}</Text>
    <Text class="text-xs text-muted-foreground">本地曲库文件保留在原文件夹，清理仅移除媒体缓存。</Text>
    <View class="flex-row gap-2">
      <Button label="清理缓存…" onPress={() => confirming.set(true)} />
      <Button label="管理文件夹…" onPress={props.onLibrary} />
    </View>
    <If when={!!error() || !!usage.error()}><Text class="text-sm text-destructive">{error() || String(usage.error())}</Text></If>
    <Dialog open={confirming()} onOpenChange={(e) => confirming.set(e.open)} class="w-96 gap-4 p-5">
      <Text role="header" class="text-lg font-semibold text-foreground">清理媒体缓存？</Text>
      <Text class="text-sm text-muted-foreground">清理后需要重新缓存在线音频。本地原始文件不会被删除。</Text>
      <View class="flex-row justify-end gap-2"><Button label="取消" disabled={busy()} onPress={() => confirming.set(false)} />
        <Button label="清理" variant="destructive" loading={busy()} onPress={() => void clear()} /></View>
    </Dialog>
  </View>;
}
