import { signal, Text, View } from "nsh";
import { Button } from "../ui/Button";
import { Field } from "../ui/LibraryControls";
import { SettingSelect, SettingSwitch } from "../ui/SettingsControls";
import { lyricsProviders, type createSettings, type CustomLyricsProvider } from "../state/settings";

/** Settings are persisted by Rust; the editable custom-provider form is a local draft. */
export function LyricsSettings(props: { settings: ReturnType<typeof createSettings> }) {
  const settings = () => props.settings.value()!;
  const providers = () => lyricsProviders(settings());
  const editing = signal(false), id = signal(""), name = signal(""), url = signal("");
  const response = signal("text"), path = signal(""), headers = signal("{}"), error = signal("");
  const custom = (key: string) => settings().lyricsCustomProviders.find(p => p.id === key);
  function edit(provider?: CustomLyricsProvider) {
    id.set(provider?.id ?? ""); name.set(provider?.name ?? "");
    url.set(provider?.urlTemplate ?? ""); response.set(provider?.responseType ?? "text");
    path.set(provider?.jsonPath ?? "");
    headers.set(JSON.stringify(provider?.headers ?? {})); error.set(""); editing.set(true);
  }
  function move(key: string, offset: number) {
    const order = providers().map(p => p.id), index = order.indexOf(key);
    const target = index + offset;
    if (target < 0 || target >= order.length) return;
    [order[index], order[target]] = [order[target], order[index]];
    void props.settings.update({ lyricsProviderOrder: order });
  }
  async function save() {
    try {
      if (!name().trim()) throw new Error("请输入来源名称");
      const parsed = new URL(url().replace(/\{[^}]+\}/g, "value"));
      if (!["http:", "https:"].includes(parsed.protocol)) throw new Error("URL 必须使用 HTTP 或 HTTPS");
      const values = JSON.parse(headers());
      if (!values || Array.isArray(values) || typeof values !== "object"
        || Object.values(values).some(v => typeof v !== "string")) {
        throw new Error("Headers 必须是字符串值的 JSON 对象");
      }
      const provider: CustomLyricsProvider = {
        id: id() || `custom:${Date.now()}`, name: name().trim(), urlTemplate: url().trim(),
        responseType: response(), jsonPath: path().trim(), headers: values,
      };
      await props.settings.update({ lyricsCustomProviders: [
        ...settings().lyricsCustomProviders.filter(p => p.id !== provider.id), provider,
      ] });
      if (!props.settings.error()) editing.set(false);
    } catch (e) { error.set(String(e)); }
  }
  async function remove(key: string) {
    await props.settings.update({
      lyricsCustomProviders: settings().lyricsCustomProviders.filter(p => p.id !== key),
      lyricsProviderOrder: settings().lyricsProviderOrder.filter(p => p !== key),
      lyricsProvidersDisabled: settings().lyricsProvidersDisabled.filter(p => p !== key),
    });
  }
  return <View testID="lyrics-settings" class="gap-4">
    <Text role="header" class="text-lg font-semibold text-foreground">歌词</Text>
    <SettingSwitch label="自动查找远端歌词" checked={settings().lyricsAutoSearch}
      onChange={lyricsAutoSearch => void props.settings.update({ lyricsAutoSearch })} />
    <Text class="text-sm text-muted-foreground">本地文件与音乐来源优先；远端按以下顺序查找，逐字歌词优先。</Text>
    <View class="gap-2">
      <Each of={providers()} key={p => p.id}>{(p, index) =>
        <View class="flex-row items-center gap-2 py-1">
          <View class="flex-1 gap-1">
            <Text class="text-sm font-semibold text-foreground">{p().name}</Text>
            <Text class="text-xs text-muted-foreground">{custom(p().id) ? "自定义 HTTP" : "内置提供方"}</Text>
          </View>
          <If when={!!custom(p().id)}>
            <Button label="编辑" accessibilityLabel={`编辑 ${p().name}`} size="sm"
              onPress={() => edit(custom(p().id))} />
            <Button label="删除" accessibilityLabel={`删除 ${p().name}`} size="sm"
              onPress={() => void remove(p().id)} />
          </If>
          <Button label="上移" accessibilityLabel={`上移 ${p().name}`} size="sm"
            disabled={index() === 0} onPress={() => move(p().id, -1)} />
          <Button label="下移" accessibilityLabel={`下移 ${p().name}`} size="sm"
            disabled={index() === providers().length - 1} onPress={() => move(p().id, 1)} />
          <SettingSwitch label={`启用 ${p().name}`} hideLabel
            checked={!settings().lyricsProvidersDisabled.includes(p().id)} onChange={enabled =>
              void props.settings.update({ lyricsProvidersDisabled: enabled
                ? settings().lyricsProvidersDisabled.filter(id => id !== p().id)
                : [...settings().lyricsProvidersDisabled, p().id] })} />
        </View>
      }</Each>
    </View>
    <View class="flex-row"><Button label="添加 HTTP 来源" onPress={() => edit()} /></View>
    <If when={editing()}><View class="gap-3 border-t border-border pt-4">
      <Text class="text-sm font-semibold text-foreground">{id() ? "编辑 HTTP 来源" : "新建 HTTP 来源"}</Text>
      <Field label="来源名称" value={name()} onChange={v => name.set(v)} />
      <Field label="URL 模板" value={url()} onChange={v => url.set(v)} placeholder="https://example.com/lyrics?title={title}" />
      <Text class="text-xs text-muted-foreground">模板变量：{"{title}、{artist}、{album}、{duration}"}，请求时自动编码。</Text>
      <SettingSelect label="响应类型" value={response()} items={[
        { value: "text", title: "文本 / LRC" }, { value: "json", title: "JSON" },
      ]} onChange={v => response.set(v)} />
      <If when={response() === "json"}><Field label="JSON 路径" value={path()} onChange={v => path.set(v)} placeholder="data.lyrics.0" /></If>
      <Field label="Headers（JSON）" value={headers()} onChange={v => headers.set(v)} />
      <If when={!!error()}><Text class="text-sm text-destructive">{error()}</Text></If>
      <View class="flex-row gap-2">
        <Button label="保存来源" variant="primary" onPress={() => void save()} />
        <Button label="取消编辑" onPress={() => editing.set(false)} />
      </View>
    </View></If>
  </View>;
}
