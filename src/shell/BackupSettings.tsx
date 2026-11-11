import { effect, signal, Text, View } from "nsh";
import { BackupService, type Core } from "haiku/native";
import { Button } from "../ui/Button";
import { SettingSwitch } from "../ui/SettingsControls";
import { Dialog } from "../ui/Surfaces";
import { localDateTime } from "../ui/dateTime";

interface Preview {
  meta: {
    schemaVersion: number; appName: string; appVersion: string; createdAt: string;
    platform: string; includesToken: boolean; includes: string[];
  };
  legacyPlugins: string[];
}
interface RestoreResult {
  safetyBackup: string;
  legacyPlugins: string[];
  warnings: string[];
}

/** The preview retains inspected bytes in Rust; confirmation never rereads a changed file. */
export function BackupSettings(props: {
  core: Core; fixturePath?: string; initialDialog?: "export" | "import";
}) {
  const exporting = signal(props.initialDialog === "export");
  const includeToken = signal(false), busy = signal(false), error = signal("");
  const selected = signal(""), preview = signal<Preview | null>(null);
  const result = signal<RestoreResult | null>(null), exported = signal("");
  async function run(action: () => Promise<void>) {
    busy.set(true); error.set("");
    try { await action(); } catch (e) { error.set(String(e)); } finally { busy.set(false); }
  }
  async function inspect() {
    const path = props.fixturePath ?? await BackupService.pickImport();
    if (!path) return;
    const next = await props.core.backup.inspect(path) as Preview;
    selected.set(path); preview.set(next); result.set(null);
  }
  effect(() => {
    if (props.initialDialog === "import") void run(inspect);
  });
  const contents = () => {
    const includes = preview()?.meta.includes ?? [];
    return [
      ["设置", "haiku_state/settings.json"], ["喜欢", "haiku_state/likes.json"],
      ["歌单与喜欢", "haiku_state/playlists.loro"], ["来源", "haiku_state/sources.json"],
      ["插件包", "cjrt_plugin_pkgs/"], ["插件实例及登录凭据", "cjrt_plugins/"],
      ["GitHub PAT", "secrets.json"],
    ].filter(([, path]) => includes.some(entry => entry.startsWith(path))).map(([name]) => name).join(" · ");
  };
  return <View testID="backup-settings" class="flex-col gap-3">
    <Text role="header" class="text-lg font-semibold text-foreground">备份与恢复</Text>
    <Text class="text-sm text-muted-foreground">
      保存设置、喜欢、歌单、来源与插件。不包含音频、缓存、字体和听歌历史。
    </Text>
    <View class="flex-row gap-2">
      <Button label="导出备份…" onPress={() => { error.set(""); exporting.set(true); }}
        disabled={busy()} />
      <Button label="导入备份…" disabled={busy()} onPress={() => void run(inspect)} />
    </View>
    <If when={!!exported()}><Text testID="backup-export-result" class="text-sm text-muted-foreground">
      {`备份已导出：${exported()}`}
    </Text></If>
    <If when={!!error()}><Text class="text-sm text-destructive">{error()}</Text></If>
    <If when={!!result()}>
      <View testID="backup-import-result" class="flex-col gap-2 rounded-(--radius) border border-border p-3">
        <Text class="text-sm font-semibold text-foreground">备份已恢复</Text>
        <Text class="text-xs text-muted-foreground">恢复前已自动保存安全备份，最多保留 3 份。</Text>
        <If when={!!result()?.legacyPlugins.length}>
          <Text class="text-sm text-foreground">
            {`以下旧版插件需要安装新版本：${result()?.legacyPlugins.join("、")}`}
          </Text>
        </If>
        <Each of={result()?.warnings ?? []} key={warning => warning}>{warning =>
          <Text class="text-sm text-muted-foreground">{warning()}</Text>
        }</Each>
      </View>
    </If>
    <Dialog open={exporting()} onOpenChange={e => { if (!busy()) exporting.set(e.open); }}
      class="w-128 max-w-full self-center mt-24 p-5 gap-4">
      <Text role="header" class="text-lg font-semibold text-foreground">导出 Haiku 备份</Text>
      <Text class="text-sm text-destructive">包含明文登录凭据</Text>
      <Text class="text-sm text-muted-foreground">
        插件登录信息会随备份保存。请妥善保管文件，仅向可信设备传输。
      </Text>
      <SettingSwitch label="包含 GitHub PAT" checked={includeToken()}
        onChange={checked => includeToken.set(checked)} />
      <Text class="text-xs text-muted-foreground">默认不包含 GitHub PAT。音频、缓存、字体和历史不会导出。</Text>
      <If when={!!error()}><Text class="text-sm text-destructive">{error()}</Text></If>
      <View class="flex-row justify-end gap-2">
        <Button label="取消导出" disabled={busy()} onPress={() => exporting.set(false)} />
        <Button label="确认导出…" variant="primary" disabled={busy()} loading={busy()}
          onPress={() => void run(async () => {
            const path = props.fixturePath ? `${props.fixturePath}.export.haikubak` : await BackupService.pickExport();
            if (!path) return;
            await props.core.backup.export(path, { includeGithubToken: includeToken() });
            exported.set(path); exporting.set(false);
          })} />
      </View>
    </Dialog>
    <Dialog open={!!preview()} onOpenChange={e => { if (!busy() && !e.open) preview.set(null); }}
      class="w-144 max-w-full self-center mt-24 p-5 gap-4">
      <Text role="header" class="text-lg font-semibold text-foreground">恢复备份预览</Text>
      <View class="flex-col gap-2">
        <Text class="text-sm text-foreground">{`创建时间：${localDateTime(preview()?.meta.createdAt ?? "")}`}</Text>
        <Text class="text-sm text-muted-foreground">
          {`${preview()?.meta.appName || "Haiku"} ${preview()?.meta.appVersion ?? ""} · ${preview()?.meta.platform ?? ""} · 格式 ${preview()?.meta.schemaVersion ?? ""}`}
        </Text>
        <Text class="text-sm text-foreground">{`包含：${contents()}`}</Text>
        <If when={!!preview()?.legacyPlugins.length}>
          <Text class="text-sm text-muted-foreground">
            {`以下旧版插件需要安装新版本：${preview()?.legacyPlugins.join("、")}`}
          </Text>
        </If>
      </View>
      <Text class="text-sm text-destructive">确认后将覆盖对应设置、歌单和插件数据。</Text>
      <Text class="text-sm text-muted-foreground">
        播放将停止并清空队列，下载与同步暂时暂停。恢复前会自动保存安全备份。
      </Text>
      <If when={!!error()}><Text class="text-sm text-destructive">{error()}</Text></If>
      <View class="flex-row justify-end gap-2">
        <Button label="取消恢复" disabled={busy()} onPress={() => preview.set(null)} />
        <Button label="确认覆盖并恢复" variant="destructive" disabled={busy()} loading={busy()}
          onPress={() => void run(async () => {
            const report = await props.core.backup.import(selected()) as RestoreResult;
            result.set(report); preview.set(null);
          })} />
      </View>
    </Dialog>
  </View>;
}
