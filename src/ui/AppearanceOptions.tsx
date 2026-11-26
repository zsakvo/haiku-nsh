import { Text, token, View } from "nsh";
import { Radio, RadioGroup } from "nsh/base";
import { brands, modes, type AppearanceMode, type createAppearance } from "./theme";
import { focus } from "./Button";
import { SettingRow, SettingSection, SettingSelect } from "./SettingsControls";
import { Tooltip } from "./Surfaces";

/** Appearance controls share the native pop-up button and 7-color macOS-style accent dots. */
export function AppearanceOptions(props: { appearance: ReturnType<typeof createAppearance> }) {
  return (
    <View class="flex-col gap-5">
      <If when={!!props.appearance.error()}>
        <Text class="text-sm text-destructive">{props.appearance.error()}</Text>
      </If>
      <SettingSection title="主题" description="深色外观会调亮主题色，保持文字与控件清晰。">
        <SettingRow title="外观模式">
          <SettingSelect label="外观模式" inline value={props.appearance.mode()}
            items={modes.map((m) => ({ value: m.id, title: m.label }))}
            onChange={(mode) => void props.appearance.setMode(mode as AppearanceMode)} />
        </SettingRow>
        <SettingRow divider title="主题色">
          <RadioGroup class="flex-row items-center gap-2">
            {brands.map(([brand, label]) => {
              const selected = () => props.appearance.brand() === brand;
              return (
                <Radio
                  accessibilityLabel={`品牌色：${label}`}
                  tooltip={<Tooltip label={label} />}
                  checked={selected()}
                  onChange={() => void props.appearance.setBrand(brand)}
                  class={`size-5 items-center justify-center rounded-full border-2 p-0.5 active:opacity-80 ${focus}`}
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
