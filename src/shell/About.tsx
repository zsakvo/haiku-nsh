import { signal, Text, View, Scroll } from "nsh";
import { Core } from "haiku/native";
import { Button } from "../ui/Button";
import { SettingRow, SettingSection } from "../ui/SettingsControls";
import { Dialog } from "../ui/Surfaces";
/** Identical software identification for Settings and the application's About command. */
export function AboutContent(props: { grouped?: boolean }) {
  const info = Core.aboutInfo() as { version: string; framework: string; license: string; notices: string };
  const legal = signal<"license" | "notices" | null>(null);
  const dialog = <Dialog open={!!legal()} onOpenChange={(e) => { if (!e.open) legal.set(null); }} class="w-192 p-5 gap-4">
    <Text role="header" class="text-lg font-semibold text-foreground">{legal() === "license" ? "许可证" : "第三方声明"}</Text>
    <Scroll class="h-96" contentClass="gap-3"><Text class="text-sm text-foreground">{legal() === "license" ? info.license : info.notices}</Text></Scroll>
    <Button label="关闭声明" onPress={() => legal.set(null)} />
  </Dialog>;
  if (props.grouped) {
    return <View testID="about-content" class="flex-col">
      <SettingSection title="关于 Haiku">
        <SettingRow title={`Haiku ${info.version} · Native Shell ${info.framework}`} description={info.license}>
          <Button label="许可证…" size="sm" onPress={() => legal.set("license")} />
          <Button label="第三方声明…" size="sm" onPress={() => legal.set("notices")} />
        </SettingRow>
      </SettingSection>
      {dialog}
    </View>;
  }
  return <View testID="about-content" class="gap-3">
    <Text role="header" class="text-lg font-semibold text-foreground">关于 Haiku</Text>
    <Text class="text-sm text-foreground">{`Haiku ${info.version} · Native Shell ${info.framework}`}</Text>
    <Text class="text-sm text-muted-foreground">{info.license}</Text>
    <View class="flex-row gap-2">
      <Button label="许可证…" size="sm" onPress={() => legal.set("license")} />
      <Button label="第三方声明…" size="sm" onPress={() => legal.set("notices")} />
    </View>
    {dialog}
  </View>;
}
