import { cleanup, effect, signal, Text, View } from "nsh";
import type { createSettings } from "../state/settings";
import { Button } from "../ui/Button";
import { Field } from "../ui/LibraryControls";
import { SettingCardBlock, SettingRow, SettingSection } from "../ui/SettingsControls";
import { Dialog } from "../ui/Surfaces";

interface AccountSnapshot {
  signedIn: boolean; baseUrl: string; codeUntil: Record<string, number>;
  user: { name?: string; nickname?: string; email?: string; id?: string | number } | null;
  subscription: { active: boolean; is_permanent?: boolean; subscription_end?: string };
  upload: { syncing: boolean; lastSync: number | null; error: string | null; retryAt: number | null };
}
interface Plan {
  id: string | number; name?: string; title?: string; price_cents?: number;
  permanent?: boolean; duration_days?: number;
}

/** Account state belongs to Rust; this panel owns only form drafts and confirmation state. */
export function AccountSettings(props: { settings: ReturnType<typeof createSettings> }) {
  const account = () => props.settings.core()!.account;
  const value = signal<AccountSnapshot | null>(null);
  const email = signal(""), code = signal(""), server = signal("");
  const now = signal(Date.now()), busy = signal(""), error = signal(""), notice = signal("");
  const confirming = signal(false), plans = signal<Plan[]>([]);
  let signedIn = false, generation = 0;
  cleanup(() => { generation++; });
  const timer = setInterval(() => now.set(Date.now()), 1000);
  cleanup(() => clearInterval(timer));

  async function loadPlans(epoch: number) {
    try {
      const next = await account().plans() as Plan[];
      if (epoch === generation) plans.set(next);
    } catch (e) {
      if (epoch === generation) error.set(`无法读取套餐：${String(e)}`);
    }
  }
  effect(() => {
    const core = props.settings.core();
    if (!core) return;
    const update = (next: unknown) => {
      const snapshot = next as AccountSnapshot;
      value.set(snapshot);
      if (snapshot.signedIn !== signedIn) {
        signedIn = snapshot.signedIn;
        generation++;
        plans.set([]);
        if (signedIn) void loadPlans(generation);
      }
    };
    const snapshot = core.account.snapshot() as AccountSnapshot;
    server.set(snapshot.baseUrl);
    update(snapshot);
    cleanup(core.account.onChange(update));
  });
  const remaining = () => Math.max(0, Math.ceil(
    ((value()?.codeUntil[email().trim().toLowerCase()] ?? 0) - now()) / 1000,
  ));
  async function run(kind: string, action: () => Promise<unknown>) {
    busy.set(kind); error.set(""); notice.set("");
    try {
      await action();
      value.set(account().snapshot() as AccountSnapshot);
    } catch (e) { error.set(String(e)); } finally { busy.set(""); }
  }
  const membership = () => {
    const subscription = value()?.subscription;
    if (!subscription?.active) return "普通账户";
    return subscription.is_permanent ? "永久会员" : "会员有效";
  };
  const syncStatus = () => {
    const snapshot = value();
    if (!snapshot?.signedIn) return "登录后自动同步听歌统计";
    if (snapshot.upload.syncing) return "正在同步听歌统计…";
    if (snapshot.upload.error) return "同步失败，稍后自动重试";
    return snapshot.upload.lastSync
      ? `最近同步：${new Date(snapshot.upload.lastSync).toLocaleString()}`
      : "等待首次同步";
  };
  return <View testID="account-settings" class="flex-col gap-5">
    <SettingSection title="Haiku 账户" description="登录后自动同步听歌统计。会员仅作展示，所有功能均可使用。">
      <If when={!value()}>
        <SettingCardBlock><Text class="text-sm text-muted-foreground">正在读取账户…</Text></SettingCardBlock>
      </If>
      <If when={!!value() && !value()!.signedIn}>
        <SettingCardBlock>
          <Field label="邮箱" value={email()} placeholder="name@example.com" onChange={v => email.set(v)} />
          <View class="flex-row items-center gap-3">
            <Button testID="account-send-code" size="sm"
              label={remaining() ? `${remaining()} 秒后重发` : "发送验证码"}
              loading={busy() === "code"} disabled={!!busy() || !!remaining() || !email().trim()}
              onPress={() => void run("code", async () => {
                await account().sendCode(email());
                now.set(Date.now()); notice.set("验证码已发送，请查看邮箱。");
              })} />
            <Text class="text-xs text-muted-foreground">首次登录将自动创建账户</Text>
          </View>
          <Field label="验证码" value={code()} placeholder="输入邮箱验证码" onChange={v => code.set(v)} />
          <View class="flex-row">
            <Button label="登录" variant="primary" loading={busy() === "login"}
              disabled={!!busy() || !email().trim() || !code().trim()}
              onPress={() => void run("login", async () => {
                await account().login(email(), code()); code.set("");
              })} />
          </View>
        </SettingCardBlock>
      </If>
      <If when={!!value()?.signedIn}>
        <View testID="account-profile" class="flex-col">
          <SettingRow
            title={value()?.user?.name || value()?.user?.nickname || value()?.user?.email || "Haiku 用户"}
            description={[
              value()?.user?.email ?? "",
              membership(),
              value()?.subscription.subscription_end && !value()?.subscription.is_permanent
                ? `有效期至 ${value()?.subscription.subscription_end}` : "",
            ].filter(Boolean).join(" · ")}>
            <Button label="刷新账户" size="sm" disabled={!!busy()} loading={busy() === "refresh"}
              onPress={() => void run("refresh", () => account().refresh())} />
            <Button label="登出…" size="sm" disabled={!!busy()} onPress={() => confirming.set(true)} />
          </SettingRow>
          <Each of={plans()} key={plan => String(plan.id)}>{plan =>
            <SettingRow divider
              title={plan().name || plan().title || String(plan().id)}
              description={[
                typeof plan().price_cents === "number" ? `¥${(plan().price_cents! / 100).toFixed(2)}` : "",
                plan().permanent ? "永久会员" : plan().duration_days ? `${plan().duration_days} 天` : "",
              ].filter(Boolean).join(" · ")}>
              <Button label="购买即将开放" size="sm" disabled onPress={() => {}} />
            </SettingRow>
          }</Each>
          <If when={!plans().length}>
            <SettingCardBlock divider><Text class="text-sm text-muted-foreground">购买即将开放</Text></SettingCardBlock>
          </If>
        </View>
      </If>
    </SettingSection>
    <SettingSection title="听歌统计同步">
      <SettingCardBlock>
        <Text testID="account-sync-status" class="text-sm text-foreground">{syncStatus()}</Text>
        <If when={!!value()?.signedIn && !!value()?.upload.error}>
          <Text class="text-sm text-destructive">{value()?.upload.error}</Text>
          <If when={!!value()?.upload.retryAt}><Text class="text-xs text-muted-foreground">
            {`下次重试：${new Date(value()!.upload.retryAt!).toLocaleString()}`}
          </Text></If>
        </If>
      </SettingCardBlock>
    </SettingSection>
    <SettingSection title="服务地址" description="更换服务器会登出当前账户，并重置远程统计的同步状态。">
      <SettingCardBlock>
        <View class="flex-row items-end gap-2">
          <View class="min-w-0 flex-1">
            <Field label="服务地址" value={server()} onChange={v => server.set(v)} />
          </View>
          <Button label="保存服务地址" disabled={!!busy() || !value() || server() === value()?.baseUrl}
            loading={busy() === "server"} onPress={() => void run("server", async () => {
              await account().setServer(server()); server.set((account().snapshot() as AccountSnapshot).baseUrl);
              notice.set("服务地址已保存。");
            })} />
        </View>
      </SettingCardBlock>
    </SettingSection>
    <If when={!!notice()}><Text class="text-sm text-muted-foreground">{notice()}</Text></If>
    <If when={!!error()}><Text class="text-sm text-destructive">{error()}</Text></If>
    <Dialog open={confirming()} onOpenChange={e => { if (!busy()) confirming.set(e.open); }}
      class="w-112 p-5 gap-4">
      <Text role="header" class="text-lg font-semibold text-foreground">登出 Haiku 账户？</Text>
      <Text class="text-sm text-muted-foreground">本机登录凭据将被清除，听歌统计停止同步。本地听歌记录与服务器上的数据会保留。</Text>
      <If when={!!error()}><Text class="text-sm text-destructive">{error()}</Text></If>
      <View class="flex-row justify-end gap-2">
        <Button label="取消登出" disabled={busy() !== ""} onPress={() => confirming.set(false)} />
        <Button label="确认登出" variant="destructive" loading={busy() === "logout"}
          disabled={!!busy()} onPress={() => void run("logout", async () => {
            await account().logout(); confirming.set(false);
          })} />
      </View>
    </Dialog>
  </View>;
}
