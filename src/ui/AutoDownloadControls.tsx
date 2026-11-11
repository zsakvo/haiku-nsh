import { cleanup, signal, Text, View } from "nsh";
import type { Core } from "haiku/native";
import type { SettingsSnapshot } from "../state/settings";
import { SettingSwitch } from "./SettingsControls";

/** Both local and favorite details use the same durable, reference-compatible settings. */
export function AutoDownloadControls(props: { core: Core; id: string }) {
  const value = signal(props.core.settings.get() as SettingsSnapshot);
  cleanup(props.core.settings.onChange(next => value.set(next as SettingsSnapshot)));
  const error = signal("");
  const enabled = () => (value().autoDownloadPlaylists ?? []).includes(props.id);
  async function update(patch: unknown) {
    try { await props.core.settings.update(patch); error.set(""); }
    catch (e) { error.set(String(e)); }
  }
  return <View class="flex-col gap-2 items-start" testID="auto-download-controls">
    <View class="flex-row gap-5 items-center">
      <SettingSwitch label="自动下载" checked={enabled()} onChange={checked => void update({
        autoDownloadPlaylists: checked
          ? [...(value().autoDownloadPlaylists ?? []), props.id]
          : (value().autoDownloadPlaylists ?? []).filter(id => id !== props.id),
      })} />
      <If when={enabled()}>
        <SettingSwitch label="跟随移除" checked={value().downloadFollowPlaylistRemoval ?? false}
          onChange={downloadFollowPlaylistRemoval => void update({ downloadFollowPlaylistRemoval })} />
      </If>
    </View>
    <If when={enabled()}><Text class="text-xs text-muted-foreground">
      新增曲目会自动下载。跟随移除适用于所有自动歌单，保留手动下载和其他自动歌单仍使用的文件。
    </Text></If>
    <If when={!!error()}><Text class="text-sm text-destructive">{error()}</Text></If>
  </View>;
}
