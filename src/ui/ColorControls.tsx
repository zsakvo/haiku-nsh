import { signal, Text, token, View } from "nsh";
import { Button as BaseButton, ColorArea, ColorEyeDropper, ColorField, ColorPicker, ColorSlider, Popover, Radio, RadioGroup } from "nsh/base";
import { focus } from "./Button";
import { Icon } from "./Icon";
import { popupSurface, Tooltip } from "./Surfaces";

/** A stored `#RRGGBB` (or `system`) from a picker's hex; alpha is dropped. */
export function hex6(value: string) {
  return value.slice(0, 7).toUpperCase();
}
export const sameColor = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();

/** Preset pairings as two overlapping dots; checked while both colors match. */
export function ColorPairPresets(props: {
  label: string;
  presets: { name: string; first: string; second: string }[];
  first: string;
  second: string;
  onSelect: (first: string, second: string) => void;
}) {
  return (
    <RadioGroup class="flex-row flex-wrap items-center gap-2">
      {props.presets.map((p) => {
        const checked = () => sameColor(props.first, p.first) && sameColor(props.second, p.second);
        return (
          <Radio
            accessibilityLabel={`${props.label}：${p.name}`}
            tooltip={<Tooltip label={p.name} />}
            checked={checked()}
            onChange={() => props.onSelect(p.first, p.second)}
            class={`flex-row items-center gap-1.5 rounded-full border px-2 py-1 active:opacity-80 ${focus}`}
            style={{ borderColor: checked() ? token("primary") : token("border") }}
          >
            <View class="flex-row">
              <View class="size-4 rounded-full border border-border" style={{ backgroundColor: p.first }} />
              <View class="-ml-1.5 size-4 rounded-full border border-border" style={{ backgroundColor: p.second }} />
            </View>
            <Text class="text-xs text-foreground">{p.name}</Text>
          </Radio>
        );
      })}
    </RadioGroup>
  );
}

/** A swatch trigger that opens a saturation/hue picker, a hex field and the system eyedropper. */
export function ColorPickerButton(props: {
  label: string;
  value: string;
  /** Shown when `value` is not a color, such as `system`. */
  fallback: string;
  onChange: (value: string) => void;
}) {
  const draft = signal<string | null>(null);
  const shown = () => draft() ?? (props.value.startsWith("#") ? props.value : props.fallback);
  return (
    <Popover
      placement="bottom"
      align="end"
      onClose={() => draft.set(null)}
      popupClass={`${popupSurface} w-64 gap-3 p-3`}
      trigger={
        <BaseButton accessibilityLabel={props.label} tooltip={<Tooltip label={props.label} />}
          class={`size-7 items-center justify-center rounded-full border border-border p-0.5 active:opacity-80 ${focus}`}>
          <View class="size-full rounded-full" style={{ backgroundColor: shown() }} />
        </BaseButton>
      }
    >
      <ColorPicker
        value={shown()}
        onChange={(e) => draft.set(hex6(e.formatted))}
        onChangeEnd={(e) => props.onChange(hex6(e.formatted))}
        class="flex-col gap-3"
      >
        <ColorArea accessibilityLabel={`${props.label}饱和度与亮度`} class="h-36 w-full rounded-(--radius)"
          thumbClass="size-4 rounded-full border-2 border-white shadow-sm" />
        <ColorSlider accessibilityLabel={`${props.label}色相`} channel="hue" class="h-6 w-full justify-center px-2.5"
          trackClass="h-3 rounded-full" thumbClass="size-5 rounded-full border-2 border-white shadow-sm" />
        <View class="flex-row items-center gap-2">
          <ColorField accessibilityLabel={`${props.label}十六进制`}
            class={`h-8 min-w-0 flex-1 rounded-(--radius-inset) border border-input bg-card px-2 text-sm text-foreground ${focus}`} />
          <ColorEyeDropper accessibilityLabel="从屏幕取色"
            class={`size-8 items-center justify-center rounded-(--radius) border border-border active:bg-muted disabled:hidden ${focus}`}>
            <Icon name="palette" small />
          </ColorEyeDropper>
        </View>
      </ColorPicker>
    </Popover>
  );
}
