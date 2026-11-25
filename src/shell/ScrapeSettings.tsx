import { Text, View } from "nsh";
import type { createSettings } from "../state/settings";
import type { PluginsState } from "../state/plugins";
import { Button } from "../ui/Button";
import {
  SettingCardBlock,
  SettingRow,
  SettingSection,
  SettingSelectRow,
  SettingSwitch,
  SettingSwitchRow,
} from "../ui/SettingsControls";

/** Reference-compatible candidate source preferences, colocated with storage controls. */
export function ScrapeSettings(props: { settings: ReturnType<typeof createSettings>; plugins: PluginsState }) {
  const value = () => props.settings.value()!;
  const disabled = () => value().scrapersDisabled ?? [];
  const order = () => value().scraperOrder ?? [];
  const sources = () => props.plugins.value().sources.filter(s => props.plugins.value().packages.some(
    p => p.id === s.pluginId && (p.capabilities.includes("scrape.search") ||
      value().scrapeUseMusicPlugins && p.capabilities.includes("search.track"))))
    .sort((a, b) => {
      const at = (id: string) => order().includes(id) ? order().indexOf(id) : 999;
      return at(a.sourceId) - at(b.sourceId);
    });
  function move(id: string, delta: number) {
    const ids = sources().map(s => s.sourceId), from = ids.indexOf(id), to = from + delta;
    if (to < 0 || to >= ids.length) return;
    [ids[from], ids[to]] = [ids[to], ids[from]];
    void props.settings.update({ scraperOrder: ids });
  }
  return <SettingSection testID="scrape-settings" title="元数据刮削"
    description="并发查找候选，明确匹配自动采用，其余留待审阅。音频标签需另行确认写回。">
    <SettingSelectRow label="刮削应用模式" value={value().scrapeMode ?? "replace"}
      items={[{ value: "missing", title: "只填补缺失字段" }, { value: "replace", title: "覆盖现有字段" }]}
      onChange={scrapeMode => void props.settings.update({ scrapeMode })} />
    <SettingSwitchRow divider label="使用音乐来源查找候选" checked={value().scrapeUseMusicPlugins ?? false}
      onChange={scrapeUseMusicPlugins => void props.settings.update({ scrapeUseMusicPlugins })} />
    <Each of={sources()} key={s => s.sourceId}>{(s, index) =>
      <SettingRow divider
        title={s().displayName ?? props.plugins.value().packages.find(p => p.id === s().pluginId)?.name ?? s().sourceId}
        description={s().sourceId}>
        <Button label="上移" accessibilityLabel={`上移 ${s().sourceId}`} size="sm"
          disabled={index() === 0} onPress={() => move(s().sourceId, -1)} />
        <Button label="下移" accessibilityLabel={`下移 ${s().sourceId}`} size="sm"
          disabled={index() === sources().length - 1} onPress={() => move(s().sourceId, 1)} />
        <SettingSwitch label={`启用刮削 ${s().sourceId}`} hideLabel checked={!disabled().includes(s().sourceId)}
          onChange={enabled => void props.settings.update({ scrapersDisabled: enabled
            ? disabled().filter(id => id !== s().sourceId) : [...disabled(), s().sourceId] })} />
      </SettingRow>}
    </Each>
    <If when={!sources().length}>
      <SettingCardBlock divider>
        <Text class="text-sm text-muted-foreground">请先在插件中启用刮削插件，或启用音乐来源候选。</Text>
      </SettingCardBlock>
    </If>
  </SettingSection>;
}
