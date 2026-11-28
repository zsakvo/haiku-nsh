import { cleanup, effect, signal, Text, token as themeToken, View } from "nsh";
import type { createSettings } from "../state/settings";
import { Button } from "../ui/Button";
import { Icon } from "../ui/Icon";
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

type Tone = "success" | "warning" | "destructive" | "muted";
const dot: Record<Tone, string> = {
  success: "bg-success", warning: "bg-warning", destructive: "bg-destructive", muted: "bg-muted-foreground",
};

/**
 * Rust owns synchronization; this panel retains only drafts and confirmation state.
 * The page follows the setup order: connect GitHub, choose a Gist, then sync. Later steps appear once reachable.
 */
export function SyncSettings(props: { settings: ReturnType<typeof createSettings> }) {
  const value = signal<SyncSnapshot | null>(null);
  const token = signal(""), gist = signal("");
  const pending = signal(false), error = signal(""), notice = signal("");
  const confirming = signal(false), rebinding = signal(false);
  const sync = () => props.settings.core()!.sync;
  effect(() => {
    const core = props.settings.core();
    if (!core) return;
    value.set(core.sync.snapshot() as SyncSnapshot);
    cleanup(core.sync.onChange(next => value.set(next as SyncSnapshot)));
  });
  const busy = () => pending() || !!value()?.busy;
  const connected = () => !!value()?.connected;
  const gistId = () => value()?.gistId ?? "";
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

  const status = (): { tone: Tone; title: string } => {
    const v = value();
    if (!v?.connected) return { tone: "muted", title: "未连接 GitHub" };
    if (!v.gistId) return { tone: "warning", title: "尚未选择同步位置" };
    if (busy()) return { tone: "success", title: "正在同步…" };
    if (v.error) return { tone: "destructive", title: "同步失败" };
    if (v.paused) return { tone: "warning", title: "自动同步已暂停" };
    return { tone: "success", title: "同步已开启" };
  };
  const hint = { fontSize: themeToken("setting-value-size") };
  const body = { fontSize: themeToken("control-text-size") };

  return <View testID="sync-settings" class="flex-col gap-6">
    {/* Status: what is happening now, and the one action that moves it forward. */}
    <SettingSection>
      <View class="flex-row items-center gap-3 px-3 py-3">
        <View class="size-10 items-center justify-center rounded-(--radius) bg-primary-soft">
          <Icon name="sync" class="size-5" color="primary-soft-foreground" />
        </View>
        <View class="min-w-0 flex-1 gap-0.5">
          <View class="flex-row items-center gap-1.5">
            <View class={`size-2 rounded-full ${dot[status().tone]}`} />
            <Text testID="sync-connection" class="font-semibold text-foreground" style={body}>{status().title}</Text>
          </View>
          <If when={!connected()}>
            <Text class="text-muted-foreground" style={hint}>连接后，喜欢与歌单会在你的设备间自动合并。</Text>
          </If>
          <If when={connected() && !gistId()}>
            <Text class="text-muted-foreground" style={hint}>创建或绑定一个 Gist 来保存同步数据。</Text>
          </If>
          <If when={connected() && !!gistId()}>
            <View class="flex-row flex-wrap items-center gap-1.5">
              <Text testID="sync-time" class="text-muted-foreground" style={hint}>
                {value()?.lastSynced
                  ? `最近同步：${new Date(value()!.lastSynced).toLocaleString()}` : "尚无成功同步记录"}
              </Text>
              <If when={!!value()?.revision}>
                <Text class="text-muted-foreground" style={hint}>·</Text>
                <Text testID="sync-version" class="text-muted-foreground" style={hint}>
                  {`版本：${value()?.revision}`}
                </Text>
              </If>
            </View>
          </If>
          <If when={!!value()?.error}>
            <Text class="text-destructive" style={hint}>{value()?.error}</Text>
          </If>
          <If when={!!value()?.paused}>
            <Text class="text-muted-foreground" style={hint}>请检查令牌与 Gist 权限，然后重新连接。</Text>
          </If>
          <If when={!!value()?.retryAt}>
            <Text class="text-muted-foreground" style={hint}>
              {`下次重试：${new Date(value()!.retryAt!).toLocaleString()}`}
            </Text>
          </If>
        </View>
        <If when={connected() && !!gistId()}>
          <Button label="立即同步" disabled={busy()} loading={busy()}
            onPress={() => void run(() => sync().syncNow(), "同步完成。")} />
        </If>
      </View>
    </SettingSection>

    {/* Step 1: credentials. */}
    <SettingSection title="GitHub 账户"
      description={connected() ? undefined : "需要具有 gist 权限的个人访问令牌（PAT）。令牌仅保存在系统钥匙串中，不会写入设置或同步内容。"}>
      <If when={!connected()}>
        <SettingCardBlock>
          <View class="flex-row items-end gap-2">
            <View class="min-w-0 flex-1">
              <Field label="GitHub PAT" secret value={token()} placeholder="ghp_…"
                onChange={next => token.set(next)} />
            </View>
            <Button label="保存 PAT" variant="primary" disabled={busy() || !token().trim()}
              onPress={() => void run(async () => {
                try { await sync().setToken(token()); } finally { token.set(""); }
              }, "PAT 已保存。")} />
          </View>
        </SettingCardBlock>
      </If>
      <If when={connected()}>
        <SettingRow title="GitHub"
          description={`已保存 PAT${value()?.username ? ` · ${value()!.username}` : ""}`}>
          <Button label="清除 PAT" size="sm" variant="destructive" disabled={busy()}
            onPress={() => void run(async () => {
              await sync().clearToken(); token.set("");
            }, "PAT 已清除，Gist 绑定保留。")} />
        </SettingRow>
      </If>
    </SettingSection>

    {/* Step 2: where the data lives. Shown once there is a token to use, or a binding to show. */}
    <If when={connected() || !!gistId()}>
      <SettingSection title="同步位置" description="本地修改 12 秒后自动同步，每 2 分钟检查一次远端更新。">
        <If when={!!gistId()}>
          <SettingRow title="当前 Gist" description={`Gist：${gistId()}`}>
            <If when={connected()}>
              <Button label={rebinding() ? "取消" : "更换…"} size="sm" variant="ghost" disabled={busy()}
                onPress={() => { gist.set(""); rebinding.set(!rebinding()); }} />
            </If>
          </SettingRow>
        </If>
        <If when={connected() && (!gistId() || rebinding())}>
          <If when={!gistId()}>
            <SettingRow title="新建私密 Gist" description="仅你自己可见，专用于保存同步数据">
              <Button label="创建私密 Gist" size="sm" disabled={busy()}
                onPress={() => void run(() => sync().createGist(), "私密 Gist 已创建。")} />
            </SettingRow>
          </If>
          <SettingCardBlock divider>
            <View class="flex-row items-end gap-2">
              <View class="min-w-0 flex-1">
                <Field label={gistId() ? "改用其他 Gist" : "或使用已有 Gist"} value={gist()}
                  placeholder="Gist ID 或 https://gist.github.com/…" onChange={next => gist.set(next)} />
              </View>
              <Button label="绑定 Gist" disabled={busy() || !gist().trim()}
                onPress={() => void run(async () => {
                  await sync().bindGist(gist()); gist.set(""); rebinding.set(false);
                }, "Gist 已绑定并同步。")} />
            </View>
          </SettingCardBlock>
        </If>
      </SettingSection>
    </If>

    <If when={connected() || !!gistId()}>
      <SettingSection>
        <SettingRow title="断开同步" description="清除本机令牌与 Gist 绑定。本地数据与远端 Gist 都会保留。">
          <Button label="断开同步…" size="sm" variant="destructive" disabled={busy()}
            onPress={() => confirming.set(true)} />
        </SettingRow>
      </SettingSection>
    </If>

    <If when={!!notice() || !!error()}>
      <View class="px-1.5">
        <If when={!!notice()}><Text class="text-muted-foreground" style={hint}>{notice()}</Text></If>
        <If when={!!error()}><Text class="text-destructive" style={hint}>{error()}</Text></If>
      </View>
    </If>

    <Dialog open={confirming()} onOpenChange={e => { if (!busy()) confirming.set(e.open); }}
      class="w-112 p-5 gap-4">
      <Text role="header" class="text-lg font-semibold text-foreground">断开 Gist 同步？</Text>
      <Text class="text-sm text-muted-foreground">清除本机 PAT 和 Gist 绑定。本地歌单与远端 Gist 均会保留。</Text>
      <If when={!!error()}><Text class="text-sm text-destructive">{error()}</Text></If>
      <View class="flex-row justify-end gap-2">
        <Button label="取消断开" disabled={busy()} onPress={() => confirming.set(false)} />
        <Button label="确认断开" variant="destructive" disabled={busy()}
          onPress={() => void run(async () => {
            await sync().disconnect(); token.set(""); gist.set(""); rebinding.set(false); confirming.set(false);
          }, "已断开同步。")} />
      </View>
    </Dialog>
  </View>;
}
