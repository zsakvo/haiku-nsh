import { cleanup, effect, signal, Text, View } from "nsh";
import type { createSettings } from "../state/settings";
import { Button } from "../ui/Button";
import { Field } from "../ui/LibraryControls";
import { SettingCardBlock, SettingRow, SettingSection } from "../ui/SettingsControls";
import { Dialog } from "../ui/Surfaces";

interface SyncSnapshot {
  connected: boolean;
  username: string;
  busy: boolean;
  gistId: string;
  revision: string | number;
  lastSynced: number;
  error: string;
  paused: boolean;
  retryAt: number | null;
}

/** Rust owns synchronization; this panel retains only drafts and confirmation state. */
export function SyncSettings(props: { settings: ReturnType<typeof createSettings> }) {
  const value = signal<SyncSnapshot | null>(null);
  const token = signal(""), gist = signal("");
  const pending = signal(false), error = signal(""), notice = signal("");
  const confirming = signal(false);
  const sync = () => props.settings.core()!.sync;
  effect(() => {
    const core = props.settings.core();
    if (!core) return;
    value.set(core.sync.snapshot() as SyncSnapshot);
    cleanup(core.sync.onChange(next => value.set(next as SyncSnapshot)));
  });
  const busy = () => pending() || !!value()?.busy;
  async function run(action: () => Promise<unknown>, message: string) {
    pending.set(true); error.set(""); notice.set("");
    try {
      await action();
      value.set(sync().snapshot() as SyncSnapshot);
      notice.set(message);
    } catch (e) {
      error.set(String(e));
    } finally {
      pending.set(false);
    }
  }
  return <View testID="sync-settings" class="flex-col gap-5">
    <SettingSection title="GitHub 凭据" description="在设备间合并喜欢与歌单。本地修改 12 秒后自动同步，每 2 分钟检查远端更新。令牌仅保存在系统钥匙串中，不会写入设置或同步内容。">
      <SettingRow title="连接状态"
        description={value()?.connected
          ? `已保存 PAT${value()?.username ? ` · ${value()!.username}` : ""}` : "尚未连接 GitHub"}>
        <Text testID="sync-connection" class="text-xs text-muted-foreground">
          {value()?.connected ? "已连接" : "未连接"}
        </Text>
      </SettingRow>
      <SettingCardBlock divider>
        <Field label="GitHub PAT" secret value={token()} placeholder="输入具有 Gist 权限的令牌"
          onChange={next => token.set(next)} />
        <View class="flex-row gap-2">
          <Button label="保存 PAT" variant="primary" size="sm" disabled={busy() || !token().trim()}
            onPress={() => void run(async () => {
              try { await sync().setToken(token()); } finally { token.set(""); }
            }, "PAT 已保存。")} />
          <Button label="清除 PAT" size="sm" disabled={busy() || !value()?.connected}
            onPress={() => void run(async () => {
              await sync().clearToken(); token.set("");
            }, "PAT 已清除，Gist 绑定保留。")} />
        </View>
      </SettingCardBlock>
    </SettingSection>
    <SettingSection title="同步位置">
      <If when={!!value()?.gistId}>
        <SettingRow title="当前 Gist">
          <Text testID="sync-gist" class="text-sm text-muted-foreground">{`Gist：${value()?.gistId}`}</Text>
        </SettingRow>
      </If>
      <SettingCardBlock divider={!!value()?.gistId}>
        <Field label="Gist ID 或 URL" value={gist()} placeholder="https://gist.github.com/…"
          onChange={next => gist.set(next)} />
        <View class="flex-row gap-2">
          <Button label="绑定 Gist" size="sm" disabled={busy() || !value()?.connected || !gist().trim()}
            onPress={() => void run(() => sync().bindGist(gist()), "Gist 已绑定并同步。")} />
          <Button label="创建私密 Gist" size="sm" disabled={busy() || !value()?.connected}
            onPress={() => void run(() => sync().createGist(), "私密 Gist 已创建。")} />
        </View>
      </SettingCardBlock>
    </SettingSection>
    <SettingSection title="同步状态">
      <SettingRow title="同步版本"
        description={value()?.lastSynced
          ? `最近同步：${new Date(value()!.lastSynced).toLocaleString()}` : "尚无成功同步记录"}>
        <Text testID="sync-version" class="text-sm text-muted-foreground">
          {`版本：${value()?.revision || "尚未同步"}`}
        </Text>
      </SettingRow>
      <SettingCardBlock divider>
        <Text testID="sync-time" class="text-xs text-muted-foreground">
          {value()?.lastSynced
            ? `最近同步：${new Date(value()!.lastSynced).toLocaleString()}` : "尚无成功同步记录"}
        </Text>
        <If when={busy()}><Text class="text-sm text-muted-foreground">正在同步…</Text></If>
        <If when={!!value()?.error}>
          <Text class="text-sm text-destructive">{value()?.error}</Text>
        </If>
        <If when={!!value()?.paused}>
          <Text class="text-xs text-muted-foreground">自动同步已暂停，请检查凭据与 Gist 权限后重新连接。</Text>
        </If>
        <If when={!!value()?.retryAt}>
          <Text class="text-xs text-muted-foreground">
            {`下次重试：${new Date(value()!.retryAt!).toLocaleString()}`}
          </Text>
        </If>
        <View class="flex-row gap-2">
          <Button label="立即同步" size="sm" disabled={busy() || !value()?.connected || !value()?.gistId}
            loading={busy()} onPress={() => void run(() => sync().syncNow(), "同步完成。")} />
          <Button label="断开同步…" size="sm" disabled={busy() || (!value()?.connected && !value()?.gistId)}
            onPress={() => confirming.set(true)} />
        </View>
      </SettingCardBlock>
    </SettingSection>
    <If when={!!notice()}><Text class="text-sm text-muted-foreground">{notice()}</Text></If>
    <If when={!!error()}><Text class="text-sm text-destructive">{error()}</Text></If>
    <Dialog open={confirming()} onOpenChange={e => { if (!busy()) confirming.set(e.open); }}
      class="w-112 max-w-full self-center mt-24 p-5 gap-4">
      <Text role="header" class="text-lg font-semibold text-foreground">断开 Gist 同步？</Text>
      <Text class="text-sm text-muted-foreground">清除本机 PAT 和 Gist 绑定。本地歌单与远端 Gist 均会保留。</Text>
      <If when={!!error()}><Text class="text-sm text-destructive">{error()}</Text></If>
      <View class="flex-row justify-end gap-2">
        <Button label="取消断开" disabled={busy()} onPress={() => confirming.set(false)} />
        <Button label="确认断开" variant="destructive" disabled={busy()}
          onPress={() => void run(async () => {
            await sync().disconnect(); token.set(""); gist.set(""); confirming.set(false);
          }, "已断开同步。")} />
      </View>
    </Dialog>
  </View>;
}
