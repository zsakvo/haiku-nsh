import { resource, signal, Text, View } from "nsh";
import { Core } from "haiku/native";
import { AppShell } from "../../shell/AppShell";
import { Button } from "../../ui/Button";
/** Headless settings persistence: isolated Core never installs a native status item. */
export function MenubarSettingsFixture() {
  const persisted=signal("");
  const ready=resource(async()=>await Core.openIsolated());
  return <View class="flex-1"><If when={!!ready()}>
    <AppShell initialMode="light" initialOverlay="settings" initialSettingsCategory="歌词" />
    <Button label="重开并读取菜单栏设置" onPress={()=>void (async()=>{
      await ready()!.close();
      const core=await Core.open();
      const s=core.settings.get() as {menubarLyricsVisible:boolean;menubarLyricHighlight:string};
      persisted.set(`菜单栏持久化 ${s.menubarLyricsVisible} ${s.menubarLyricHighlight}`);
    })()} />
    <Text>{persisted()}</Text>
  </If><If when={!!ready.error()}><Text>{String(ready.error())}</Text></If></View>;
}
