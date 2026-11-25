import { platform, signal, Text, token, View, type ViewRef } from "nsh";
import { Button as BaseButton, Radio, RadioGroup } from "nsh/base";
import { isAvailable as desktopAvailable, Menu as DesktopMenu } from "nsh/desktop";
import { brands, modes, type AppearanceMode, type createAppearance } from "./theme";
import { focus } from "./Button";
import { SettingRow, SettingSection, SettingSelect } from "./SettingsControls";
import { Tooltip } from "./Surfaces";

function ModeMenu(props: { value: AppearanceMode; onChange: (mode: AppearanceMode) => void }) {
  const nativeMenu = platform.os === "macos" && desktopAvailable;
  const currentLabel = () => modes.find((m) => m.id === props.value)?.label ?? "跟随系统";

  if (nativeMenu) {
    const open = signal(false);
    let anchor: ViewRef | undefined;
    async function openMenu() {
      if (open()) return;
      open.set(true);
      try {
        const measured = await anchor?.measure();
        const at = measured
          ? { x: measured.pagePosition.x, y: measured.pagePosition.y + measured.bounds.size.height + 4 }
          : undefined;
        await DesktopMenu.popup(
          modes.map((mode) => ({
            label: mode.label,
            checked: mode.id === props.value,
            onSelect: () => {
              if (mode.id !== props.value) props.onChange(mode.id);
            },
          })),
          at,
        );
      } finally {
        open.set(false);
      }
    }
    return (
      <View ref={(node) => { anchor = node; }} class="shrink-0">
        <BaseButton
          accessibilityLabel="外观模式"
          selected={open()}
          onPress={() => void openMenu()}
          class={`min-h-7 flex-row items-center gap-1.5 rounded-(--radius) px-2 py-0.5 active:bg-muted ${focus}`}
        >
          <Text class="text-muted-foreground" style={{ fontSize: token("sidebar-text-size") }}>{currentLabel()}</Text>
          <Text class="text-muted-foreground" style={{ fontSize: token("sidebar-hint-size") }}>⌄</Text>
        </BaseButton>
      </View>
    );
  }

  return (
    <SettingSelect
      label="外观模式"
      inline
      value={props.value}
      items={modes.map((m) => ({ value: m.id, title: m.label }))}
      onChange={(v) => props.onChange(v as AppearanceMode)}
    />
  );
}

/** Appearance controls share native mode menu and 7-color macOS-style accent dots. */
export function AppearanceOptions(props: { appearance: ReturnType<typeof createAppearance> }) {
  return (
    <View class="flex-col gap-5">
      <If when={!!props.appearance.error()}>
        <Text class="text-sm text-destructive">{props.appearance.error()}</Text>
      </If>
      <SettingSection title="主题" description="深色外观会调亮主题色，保持文字与控件清晰。">
        <SettingRow title="外观模式">
          <ModeMenu value={props.appearance.mode()} onChange={(mode) => void props.appearance.setMode(mode)} />
        </SettingRow>
        <SettingRow divider title="主题色">
          <RadioGroup class="flex-row items-center gap-1.5">
            {brands.map(([brand, label]) => {
              const selected = () => props.appearance.brand() === brand;
              return (
                <Radio
                  accessibilityLabel={`品牌色：${label}`}
                  tooltip={<Tooltip label={label} />}
                  checked={selected()}
                  onChange={() => void props.appearance.setBrand(brand)}
                  class={`size-6 items-center justify-center rounded-full border-2 p-0.5 active:opacity-80 ${focus}`}
                  style={{ borderColor: selected() ? token(`brand-${brand}`) : token("transparent") }}
                >
                  <View
                    class="size-full rounded-full"
                    style={{ backgroundColor: token(`brand-${brand}`) }}
                  />
                </Radio>
              );
            })}
          </RadioGroup>
        </SettingRow>
      </SettingSection>
    </View>
  );
}
