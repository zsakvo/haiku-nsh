import { platform, signal, Text, View } from "nsh";
import type { createSettings } from "../state/settings";
import { SettingSelect, SettingSlider, SettingSwitch } from "../ui/SettingsControls";
import { Button } from "../ui/Button";
import { Field } from "../ui/LibraryControls";
import { Dialog } from "../ui/Surfaces";
const presets = [
  { id: "flat", name: "平坦", gains: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0] },
  { id: "pop", name: "流行", gains: [-1, 0, 2, 3, 2, 0, -1, -1, 1, 2] },
  { id: "rock", name: "摇滚", gains: [4, 3, 1, -1, -2, 0, 2, 3, 4, 4] },
  { id: "jazz", name: "爵士", gains: [3, 2, 1, 2, -1, -1, 0, 1, 2, 3] },
  { id: "classical", name: "古典", gains: [3, 2, 0, 0, 0, 0, -1, -1, -2, -3] },
  { id: "vocal", name: "人声", gains: [-2, -2, -1, 2, 4, 4, 3, 1, 0, -1] },
  { id: "bass", name: "低音增强", gains: [7, 6, 4, 2, 0, 0, 0, 0, 0, 0] },
  { id: "treble", name: "高音增强", gains: [0, 0, 0, 0, 0, 1, 2, 4, 5, 6] },
];
const frequencies = [
  "31 Hz",
  "62 Hz",
  "125 Hz",
  "250 Hz",
  "500 Hz",
  "1 kHz",
  "2 kHz",
  "4 kHz",
  "8 kHz",
  "16 kHz",
];
export function PlaybackSettings(props: { settings: ReturnType<typeof createSettings> }) {
  const e = () => props.settings.value()!.audioEffects;
  const update = (patch: unknown) => void props.settings.update({ audioEffects: patch });
  const name = signal("");
  const deleting = signal<string | null>(null);
  const allPresets = () => [...presets, ...e().customPresets];
  const currentPreset = () => e().customPresets.find((p) => p.id === e().presetId);
  return (
    <View class="gap-4" testID="playback-settings">
      <Text role="header" class="text-lg font-semibold text-foreground">
        播放
      </Text>
      <View class="gap-2">
        <SettingSwitch
          label="音效"
          checked={e().enabled}
          onChange={(enabled) => update({ enabled })}
        />
        <SettingSelect
          label="均衡器预设"
          value={e().presetId}
          items={[
            ...allPresets().map((p) => ({ value: p.id, title: p.name })),
            { value: "", title: "自定义" },
          ]}
          onChange={(id) => {
            const p = allPresets().find((p) => p.id === id);
            if (p) update({ presetId: id, bandGainsDb: p.gains });
          }}
        />
        <View class="grid grid-cols-5 gap-x-4 gap-y-2">
          {frequencies.map((label, i) => (
            <SettingSlider
              label={label}
              testID={`eq-band-${i}`}
              value={e().bandGainsDb[i]}
              min={-12}
              max={12}
              onChange={(value) => {
                const gains = [...e().bandGainsDb];
                gains[i] = value;
                update({ bandGainsDb: gains, presetId: currentPreset()?.id ?? "" });
              }}
            />
          ))}
        </View>
        <View class="flex-row items-end gap-2">
          <Field label="预设名称" value={name()} onChange={name.set} placeholder="自定义预设名称" />
          <Button
            label="另存预设"
            disabled={!name().trim()}
            onPress={() => {
              const id = `custom:${Date.now()}`;
              update({
                presetId: id,
                customPresets: [
                  ...e().customPresets,
                  { id, name: name().trim(), gains: e().bandGainsDb },
                ],
              });
              name.set("");
            }}
          />
          <Button
            label="覆盖当前预设"
            disabled={!currentPreset()}
            onPress={() => {
              const current = currentPreset();
              if (!current) return;
              update({
                customPresets: e().customPresets.map((p) =>
                  p.id === current.id
                    ? { ...p, name: name().trim() || p.name, gains: [...e().bandGainsDb] }
                    : p,
                ),
              });
            }}
          />
          <Button
            label="删除预设…"
            disabled={!currentPreset()}
            onPress={() => deleting.set(e().presetId)}
          />
        </View>
        <View class="flex-row">
          <Button
            label="重置音效"
            onPress={() =>
              update({
                bandGainsDb: Array(10).fill(0),
                presetId: "flat",
                autoPreamp: true,
                preampDb: 0,
                stereoWidth: 1,
                balance: 0,
                mono: false,
              })
            }
          />
        </View>
      </View>
      <View class="gap-2">
        <SettingSwitch
          label="自动前级增益"
          checked={e().autoPreamp}
          onChange={(autoPreamp) => update({ autoPreamp })}
        />
        <View class="grid grid-cols-3 gap-4">
          <SettingSlider
            label="前级增益 · dB"
            value={e().preampDb}
            min={-12}
            max={12}
            disabled={e().autoPreamp}
            onChange={(preampDb) => update({ preampDb })}
          />
          <SettingSlider
            label="声场宽度"
            value={e().stereoWidth}
            min={0}
            max={2}
            step={0.1}
            disabled={e().mono}
            onChange={(stereoWidth) => update({ stereoWidth })}
          />
          <SettingSlider
            label="左右平衡"
            value={e().balance}
            min={-1}
            max={1}
            step={0.1}
            onChange={(balance) => update({ balance })}
          />
        </View>
        <SettingSwitch label="单声道" checked={e().mono} onChange={(mono) => update({ mono })} />
        <Text class="text-xs text-muted-foreground">软削波与 8 毫秒渐入渐出始终保护音频输出。</Text>
        <SettingSwitch
          label="响度均衡"
          checked={props.settings.value()!.volumeNormalization}
          onChange={(volumeNormalization) => void props.settings.update({ volumeNormalization })}
        />
        <Text class="text-xs text-muted-foreground">EBU R128 · 目标 −18 LUFS · 保留峰值余量</Text>
        <If when={platform.os === "macos"}>
          <SettingSwitch
            label="正在播放时保持屏幕常亮"
            checked={props.settings.value()!.preventScreenSleepOnPlayer}
            onChange={(preventScreenSleepOnPlayer) =>
              void props.settings.update({ preventScreenSleepOnPlayer })
            }
          />
        </If>
      </View>
      <Dialog
        open={!!deleting()}
        onOpenChange={(event) => {
          if (!event.open) deleting.set(null);
        }}
        class="w-96 p-5 gap-4"
      >
        <Text role="header" class="text-lg font-semibold text-foreground">
          删除预设？
        </Text>
        <Text class="text-sm text-muted-foreground">
          将删除“{e().customPresets.find((p) => p.id === deleting())?.name}”，当前均衡曲线会保留。
        </Text>
        <View class="flex-row justify-end gap-2">
          <Button label="取消" onPress={() => deleting.set(null)} />
          <Button
            label="确认删除预设"
            variant="destructive"
            onPress={async () => {
              await props.settings.update({
                audioEffects: {
                  presetId: e().presetId === deleting() ? "" : e().presetId,
                  customPresets: e().customPresets.filter((p) => p.id !== deleting()),
                },
              });
              if (!props.settings.error()) deleting.set(null);
            }}
          />
        </View>
      </Dialog>
    </View>
  );
}
