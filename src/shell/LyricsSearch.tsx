import { cleanup, effect, List, Scroll, signal, Text, View } from "nsh";
import { Button as BaseButton } from "nsh/base";
import type { Core } from "haiku/native";
import { Button, focus } from "../ui/Button";
import { Field } from "../ui/LibraryControls";
import { Dialog } from "../ui/Surfaces";
import { SettingSelect } from "../ui/SettingsControls";
import { lyricsProviders, withLyricsDefaults, type SettingsSnapshot } from "../state/settings";

/** Drafts and preview selection belong to this dialog; Rust owns results and cancellation. */
export function LyricsSearch(props: { core: Core; open: boolean; onClose: () => void }) {
  const keyword = signal(""), artist = signal(""), source = signal("");
  const candidates = signal<any[]>([]), selected = signal<any>(null);
  const preview = signal<{ lines: { text: string; translation?: string }[] } | null>(null);
  const error = signal(""), sourceErrors = signal<Record<string, string>>({});
  const searching = signal(false), resolving = signal(false), saving = signal(false);
  const snapshot = signal<any>(props.core.lyrics.snapshot());
  const settings = signal(withLyricsDefaults(props.core.settings.get() as SettingsSnapshot));
  cleanup(props.core.settings.onChange(s => settings.set(withLyricsDefaults(s as SettingsSnapshot))));
  cleanup(props.core.lyrics.onChange(s => snapshot.set(s)));
  let searchVersion = 0, previewVersion = 0;
  let identity = "";
  const enabled = () => lyricsProviders(settings()).filter(p =>
    !settings().lyricsProvidersDisabled.includes(p.id));
  function cancel() {
    searchVersion++;
    previewVersion++;
    props.core.lyrics.cancelPreview();
    resolving.set(false); searching.set(false);
  }
  cleanup(cancel);
  effect(() => {
    const track = snapshot().track;
    const nextIdentity = JSON.stringify([props.open, track]);
    if (identity === nextIdentity) return;
    identity = nextIdentity;
    if (!props.open) { cancel(); return; }
    keyword.set(track?.title ?? "");
    artist.set((track?.artists ?? []).map((a: any) => typeof a === "string" ? a : a.name).join(", "));
    candidates.set([]); selected.set(null); preview.set(null); error.set(""); sourceErrors.set({});
    // Track identity also invalidates a dialog left open across a queue change.
    cancel();
    if (keyword()) void search();
  });
  async function choose(candidate: any) {
    props.core.lyrics.cancelPreview();
    const version = ++previewVersion;
    selected.set(candidate); preview.set(null); resolving.set(true); error.set("");
    try {
      const result = await props.core.lyrics.resolve(candidate);
      if (props.open && version === previewVersion) preview.set(result as NonNullable<ReturnType<typeof preview>>);
    } catch (e) {
      if (props.open && version === previewVersion) error.set(String(e));
    } finally {
      if (version === previewVersion) resolving.set(false);
    }
  }
  async function search() {
    cancel();
    const version = searchVersion;
    searching.set(true); error.set(""); sourceErrors.set({});
    candidates.set([]); selected.set(null); preview.set(null);
    try {
      const report = await props.core.lyrics.searchReport({
        title: keyword().trim(), artist: artist().trim(),
        album: snapshot().track?.album?.title, durationSec: snapshot().track?.duration ?? -1,
      }, source() ? [source()] : enabled().map(p => p.id)) as any;
      if (!props.open || version !== searchVersion) return;
      candidates.set(report.candidates); sourceErrors.set(report.sourceErrors);
      if (report.candidates.length) void choose(report.candidates[0]);
    } catch (e) {
      if (props.open && version === searchVersion) error.set(String(e));
    } finally {
      if (version === searchVersion) searching.set(false);
    }
  }
  async function apply(automatic = false) {
    saving.set(true); error.set("");
    try {
      if (automatic) await props.core.lyrics.resetToAuto();
      else await props.core.lyrics.adopt(selected());
      cancel(); props.onClose();
    } catch (e) { error.set(String(e)); }
    finally { saving.set(false); }
  }
  return <Dialog open={props.open} onOpenChange={e => {
    if (!e.open) { cancel(); props.onClose(); }
  }} class="w-224 max-w-full self-center mt-12 p-5 gap-4">
    <View class="flex-row items-center justify-between">
      <Text role="header" class="text-lg font-semibold text-foreground">查找歌词</Text>
      <Button label="关闭查找" size="sm" variant="ghost" onPress={() => { cancel(); props.onClose(); }} />
    </View>
    <View class="flex-row items-end gap-3">
      <View class="flex-1"><Field label="歌名 / 关键词" value={keyword()} onChange={v => keyword.set(v)} /></View>
      <View class="flex-1"><Field label="歌手" value={artist()} onChange={v => artist.set(v)} /></View>
      <Button label="搜索歌词" variant="primary" loading={searching()}
        disabled={!keyword().trim()} onPress={() => void search()} />
    </View>
    <SettingSelect label="歌词来源" value={source()} items={[
      { value: "", title: "全部已启用来源" }, ...enabled().map(p => ({ value: p.id, title: p.name })),
    ]} onChange={v => source.set(v)} />
    <If when={!!error()}><Text class="text-sm text-destructive">{error()}</Text></If>
    <Each of={Object.entries(sourceErrors())} key={e => e[0]}>{entry =>
      <Text class="text-xs text-destructive">{`${lyricsProviders(settings()).find(p =>
        p.id === entry()[0])?.name ?? entry()[0]}：${entry()[1]}`}</Text>
    }</Each>
    <View class="h-72 min-h-0 flex-row gap-4">
      <View class="flex-1 min-w-0 gap-2">
        <Text class="text-xs text-muted-foreground">{`候选 ${candidates().length} 项`}</Text>
        <List testID="lyrics-candidates" each={candidates()} keyExtractor={c =>
          `${c.providerId}:${c.candidateId}`} estimatedItemSize={76} class="flex-1">
          {c => <BaseButton accessibilityLabel={`预览 ${c().title} ${c().sourceLabel || c().providerId}`}
            onPress={() => void choose(c())} selected={selected()?.candidateId === c().candidateId
              && selected()?.providerId === c().providerId}
            class={`p-3 gap-1 items-start rounded-(--radius) hover:bg-accent selected:bg-accent ${focus}`}>
            <Text class="text-sm font-semibold text-foreground">{c().title}</Text>
            <Text class="text-xs text-muted-foreground">{`${c().artist} · ${c().sourceLabel || c().providerId}`}</Text>
            <Text class="text-xs text-muted-foreground">{[
              `${c().score} 分`, c().recommended ? "推荐" : "", c().hasWordLevel ? "逐字" : "",
              c().hasTranslation ? "翻译" : "",
            ].filter(Boolean).join(" · ")}</Text>
          </BaseButton>}
        </List>
      </View>
      <View class="flex-1 min-w-0 gap-2 border-l border-border pl-4">
        <Text class="text-sm font-semibold text-foreground">歌词预览</Text>
        <If when={resolving()}><Text class="text-sm text-muted-foreground">正在加载预览…</Text></If>
        <If when={!selected()}><Text class="text-sm text-muted-foreground">搜索并选择候选，预览后采用。</Text></If>
        <Scroll testID="lyrics-preview" class="flex-1" contentClass="flex-col gap-3">
          <Each of={preview()?.lines ?? []} key={(_, i) => i}>{line =>
            <View class="gap-1"><Text class="text-sm text-foreground">{line().text}</Text>
              <If when={!!line().translation}><Text class="text-xs text-muted-foreground">{line().translation}</Text></If>
            </View>
          }</Each>
        </Scroll>
      </View>
    </View>
    <View class="flex-row items-center gap-3">
      <Button label="恢复自动" disabled={saving() || !snapshot().track} onPress={() => void apply(true)} />
      <View class="flex-1" />
      <Button label="采用歌词" variant="primary" loading={saving()}
        disabled={!preview() || resolving()} onPress={() => void apply()} />
    </View>
  </Dialog>;
}
