import { platform, signal, Text, View } from "nsh";
import type { createSettings } from "../state/settings";
import {
  EqualizerBand,
  EqualizerScale,
  SettingCardBlock,
  SettingRow,
  SettingSection,
  SettingSelect,
  SettingSliderRow,
  SettingSwitchRow,
} from "../ui/SettingsControls";
import { Button } from "../ui/Button";
import { Field } from "../ui/LibraryControls";
import { ActionMenu, Dialog } from "../ui/Surfaces";
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
/** Short labels under the faders; the unit sits once under the scale. */
const bands = ["31", "62", "125", "250", "500", "1k", "2k", "4k", "8k", "16k"];
const EQ_RANGE = 12;
const signed = (v: number, digits = 1) =>
  Math.abs(v) < 0.05 ? (0).toFixed(digits) : `${v > 0 ? "+" : "−"}${Math.abs(v).toFixed(digits)}`;
export function PlaybackSettings(props: { settings: ReturnType<typeof createSettings> }) {
  const e = () => props.settings.value()!.audioEffects;
  const update = (patch: unknown) => void props.settings.update({ audioEffects: patch });
  const name = signal("");
  const saving = signal(false);
  const deleting = signal<string | null>(null);
  const allPresets = () => [...presets, ...e().customPresets];
  const currentPreset = () => e().customPresets.find((p) => p.id === e().presetId);
  const savePreset = () => {
    const trimmed = name().trim();
    if (!trimmed) return;
    const id = `custom:${Date.now()}`;
    update({
      presetId: id,
      customPresets: [...e().customPresets, { id, name: trimmed, gains: [...e().bandGainsDb] }],
    });
    saving.set(false);
  };
  return (
    <View class="flex-col gap-6" testID="playback-settings">
      <SettingSection title="均衡器">
        <SettingSwitchRow
          label="音效"
          description="启用均衡器与声场处理"
          checked={e().enabled}
          onChange={(enabled) => update({ enabled })}
        />
        <SettingRow divider title="预设">
          <SettingSelect
            inline
            label="均衡器预设"
            value={e().presetId}
            items={allPresets().map((p) => ({ value: p.id, title: p.name }))}
            onChange={(id) => {
              const p = allPresets().find((p) => p.id === id);
              if (p) update({ presetId: id, bandGainsDb: p.gains });
            }}
          />
          <ActionMenu
            label="…"
            compact
            accessibilityLabel="预设操作"
            items={[
              {
                label: "存储为新预设…",
                onSelect: () => {
                  name.set("");
                  saving.set(true);
                },
              },
              {
                label: "覆盖当前预设",
                disabled: !currentPreset(),
                separatorBefore: true,
                onSelect: () => {
                  const current = currentPreset();
                  if (!current) return;
                  update({
                    customPresets: e().customPresets.map((p) =>
                      p.id === current.id ? { ...p, gains: [...e().bandGainsDb] } : p,
                    ),
                  });
                },
              },
              {
                label: "删除预设…",
                disabled: !currentPreset(),
                onSelect: () => deleting.set(e().presetId),
              },
            ]}
          />
        </SettingRow>
        <SettingCardBlock class="pb-4 pt-3">
          <View class="flex-row items-start gap-2">
            <EqualizerScale max={EQ_RANGE} unit="Hz" />
            <View class="min-w-0 flex-1 flex-row">
              {bands.map((label, i) => (
                <EqualizerBand
                  label={label}
                  accessibilityLabel={`${label}Hz 增益`}
                  testID={`eq-band-${i}`}
                  value={e().bandGainsDb[i]}
                  min={-EQ_RANGE}
                  max={EQ_RANGE}
                  onChange={(value) => {
                    const gains = [...e().bandGainsDb];
                    gains[i] = value;
                    update({ bandGainsDb: gains, presetId: currentPreset()?.id ?? "" });
                  }}
                />
              ))}
            </View>
          </View>
        </SettingCardBlock>
      </SettingSection>

      <SettingSection title="声音" description="软削波与 8 毫秒渐入渐出始终保护音频输出。">
        <SettingSwitchRow
          label="自动前级增益"
          description="按均衡曲线自动预留余量，避免削波"
          checked={e().autoPreamp}
          onChange={(autoPreamp) => update({ autoPreamp })}
        />
        <SettingSliderRow
          divider
          label="前级增益"
          value={e().preampDb}
          min={-12}
          max={12}
          disabled={e().autoPreamp}
          format={(v) => `${signed(v)} dB`}
          onChange={(preampDb) => update({ preampDb })}
        />
        <SettingSliderRow
          divider
          label="声场宽度"
          value={e().stereoWidth}
          min={0}
          max={2}
          step={0.1}
          disabled={e().mono}
          format={(v) => `${Math.round(v * 100)}%`}
          onChange={(stereoWidth) => update({ stereoWidth })}
        />
        <SettingSliderRow
          divider
          centered
          label="左右平衡"
          value={e().balance}
          min={-1}
          max={1}
          step={0.1}
          startLabel="左"
          endLabel="右"
          format={(v) =>
            Math.abs(v) < 0.05 ? "居中" : `${v < 0 ? "左" : "右"} ${Math.round(Math.abs(v) * 100)}%`
          }
          onChange={(balance) => update({ balance })}
        />
        <SettingSwitchRow
          divider
          label="单声道"
          description="将左右声道混合后输出"
          checked={e().mono}
          onChange={(mono) => update({ mono })}
        />
      </SettingSection>

      <SettingSection title="播放">
        <SettingSwitchRow
          label="响度均衡"
          description="让不同歌曲的音量保持一致（EBU R128，−18 LUFS）"
          checked={props.settings.value()!.volumeNormalization}
          onChange={(volumeNormalization) => void props.settings.update({ volumeNormalization })}
        />
        <If when={platform.os === "macos"}>
          <SettingSwitchRow
            divider
            label="播放时保持屏幕常亮"
            checked={props.settings.value()!.preventScreenSleepOnPlayer}
            onChange={(preventScreenSleepOnPlayer) =>
              void props.settings.update({ preventScreenSleepOnPlayer })
            }
          />
        </If>
      </SettingSection>

      <SettingSection>
        <SettingRow title="恢复默认音效" description="均衡器与声音设置恢复为默认值，自定义预设会保留。">
          <Button
            label="重置音效"
            size="sm"
            variant="destructive"
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
        </SettingRow>
      </SettingSection>

      <Dialog
        open={saving()}
        onOpenChange={(event) => {
          if (!event.open) saving.set(false);
        }}
        class="w-80 p-5 gap-4"
      >
        <Text role="header" class="text-lg font-semibold text-foreground">
          存储为新预设
        </Text>
        <Field label="名称" value={name()} onChange={name.set} placeholder="例如：夜间聆听" />
        <View class="flex-row justify-end gap-2">
          <Button label="取消" onPress={() => saving.set(false)} />
          <Button label="存储" variant="primary" disabled={!name().trim()} onPress={savePreset} />
        </View>
      </Dialog>
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
