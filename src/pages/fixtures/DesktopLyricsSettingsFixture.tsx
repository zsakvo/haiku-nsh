import { resource, signal, Text, View } from "nsh";
import { Core } from "haiku/native";
import { DesktopLyricsSettings } from "../../shell/DesktopLyricsSettings";
import { createSettings, type SettingsSnapshot } from "../../state/settings";
import { AppShell } from "../../shell/AppShell";
import { Button } from "../../ui/Button";
function Controls() {
  const settings=createSettings(),persisted=signal("");
  return <View class="flex-1 gap-3 bg-background p-4">
    <If when={!!settings.value()}><DesktopLyricsSettings settings={settings} /></If>
    <Button label="重开桌面歌词设置" onPress={()=>void (async()=>{
      await settings.core()!.close();const c=await Core.open();const d=(c.settings.get() as SettingsSnapshot).desktopLyrics;
      persisted.set(`桌面持久化 ${d.visible} ${d.locked} ${d.layout} ${d.fontWeight} ${d.background} ${d.textColor} ${d.highlightColor} ${d.outline} ${d.shadow} ${d.showTranslation}`);
    })()} />
    <Button label="快捷键切换桌面歌词" onPress={()=>void settings.core()!.shortcuts.trigger("toggleDesktopLyrics")} />
    <Text>{persisted()}</Text>
  </View>;
}
/** Isolated Core and no NSPanel, even when the visible preference is enabled. */
export function DesktopLyricsSettingsFixture(props:{shell?:boolean}={}) {
  const ready=resource(async()=>await Core.openIsolated());
  return <View class="flex-1"><If when={!!ready()}><If when={!props.shell}><Controls /></If><If when={props.shell}>
    <AppShell initialMode="light" initialOverlay="settings" initialSettingsCategory="播放" />
    <Button label="请求歌词设置" onPress={()=>ready()!.settings.openLyricsSettings()} />
  </If></If><If when={!!ready.error()}><Text>{String(ready.error())}</Text></If></View>;
}
