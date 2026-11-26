import { cleanup, effect, Image, List, platform, Scroll, signal, Text, token, View, type ViewRef } from "nsh";
import { PluginService } from "haiku/native";
import { Button as BaseButton, Popover } from "nsh/base";
import { isAvailable as desktopAvailable, Menu as DesktopMenu } from "nsh/desktop";
import { Button, focus } from "../ui/Button";
import { Icon } from "../ui/Icon";
import { Field } from "../ui/LibraryControls";
import { ActionMenu, Dialog, Menu, MenuItem, popupSurface } from "../ui/Surfaces";
import {
  SettingCardBlock,
  SettingDivider,
  SettingSection,
  SettingSelect,
  SettingSelectRow,
  SettingSwitchRow,
} from "../ui/SettingsControls";
import type { PluginDescriptor, PluginsState, SourceRecord } from "../state/plugins";

export function SourceLogin(props: { state: PluginsState; initialPlugin: string; initialSource?: SourceRecord; initialMethod?: string; lockedPlugin?: boolean; onClose: () => void }) {
  const plugin = signal(props.initialPlugin), method = signal(props.initialMethod ?? ""), values = signal<Record<string, string>>(Object.fromEntries(
    Object.entries(props.initialSource?.connection ?? {}).filter(([, value]) => value !== "[REDACTED]")
  ));
  let session: string | null = null;
  const presentation = signal<Record<string, unknown>>({});
  const phone = signal(""), code = signal(""), cookie = signal(""), pageUrl = signal("");
  const cooldown = signal(0), variant = signal(""), qrStatus = signal(""), qrExpired = signal(false);
  let closed = false, generation = 0;
  const descriptor = () => props.state.value().packages.find(p => p.id === plugin());
  const activeMethod = () => method() || descriptor()?.authMethods[0] || "";
  const fields = () => descriptor()?.connection?.fields
    ?? descriptor()?.auth?.password?.fields ?? {};
  async function cancel() {
    generation++;
    const id = session; session = null;
    if (id) await props.state.core()?.sources.cancelLogin(id);
  }
  cleanup(() => { closed = true; void cancel(); });
  effect(() => {
    const count = cooldown();
    if (count > 0) {
      const timer = setTimeout(() => cooldown.set(Math.max(0, cooldown() - 1)), 1000);
      return () => clearTimeout(timer);
    }
  });
  async function step(action: string, input: unknown) {
    return props.state.run(async c => {
      let id = session;
      if (!id) {
        id = await c.sources.beginLogin(plugin());
        if (closed) { await c.sources.cancelLogin(id); return; }
        session = id;
      }
      const result = await c.sources.loginStep(id, action, input) as {
        done: boolean; result: Record<string, unknown>;
      };
      if (result.done) { session = null; props.onClose(); }
      return result;
    });
  }
  async function startQr() {
    await cancel();
    const revision = generation;
    qrExpired.set(false);
    qrStatus.set("正在生成二维码…");
    const start = await step("qrcode.start", variant() || Object.keys(descriptor()?.auth?.qrcode?.variants ?? {})[0] || "");
    if (!start || closed || revision !== generation) return;
    presentation.set(start.result); qrStatus.set("请使用手机扫码");
    while (!closed && revision === generation && session) {
      const poll = await step("qrcode.poll", null);
      if (closed || revision !== generation) return;
      if (!poll) { qrStatus.set("登录中断，请重试"); break; }
      if (poll.done) break;
      const status = String(poll.result.status);
      qrStatus.set(status === "scanned" ? "已扫描，请在手机上确认"
        : status === "expired" ? "二维码已过期，请刷新" : "请使用手机扫码");
      if (status === "expired") { qrExpired.set(true); break; }
    }
  }
  let qrKey = "";
  effect(() => {
    if (!descriptor() || descriptor()?.connection || activeMethod() !== "qrcode") return;
    const key = `${plugin()}:${variant() || Object.keys(descriptor()?.auth?.qrcode?.variants ?? {})[0] || ""}`;
    if (key !== qrKey) { qrKey = key; void startQr(); }
  });
  async function submit() {
    if (descriptor()?.connection) {
      await props.state.run(async c => {
        const id = c.sources.beginConnection(); session = id;
        try { await c.sources.connect(plugin(), values(), props.initialSource?.displayName ?? null, id); props.onClose(); }
        finally { await c.sources.cancelLogin(id); session = null; }
      });
    } else if (activeMethod() === "web") {
      const cookies = cookie().split(';').map(part => {
        const at = part.indexOf('=');
        return { name: part.slice(0, at).trim(), value: part.slice(at + 1).trim() };
      }).filter(part => part.name);
      await step("web", { url: pageUrl() || descriptor()?.auth?.web?.url, cookies });
    } else if (activeMethod() === "cookie") await step("cookie", cookie());
    else if (activeMethod() === "sms") await step("sms.verify", code());
    else await step("password", values());
  }
  const heading = () => {
    if (props.initialSource) return "编辑连接";
    if (props.lockedPlugin) {
      const name = descriptor()?.name;
      return name ? `登录 ${name}` : "登录";
    }
    return "添加新来源";
  };
  const submitLabel = () => {
    if (props.initialSource) return "验证并保存";
    if (props.lockedPlugin) return descriptor()?.connection ? "连接" : "登录";
    return "连接并添加";
  };
  return <View class="gap-4" testID="source-login">
    <Text role="header" class="text-lg font-semibold text-foreground">{heading()}</Text>
    <If when={!props.initialSource && !props.lockedPlugin}><SettingSelect label="插件" value={plugin()} items={props.state.value().packages
      .filter(p => p.kind !== "scraper").map(p => ({ value: p.id, title: p.name }))}
      onChange={id => { void cancel(); plugin.set(id); values.set({}); method.set(""); }} /></If>
    <If when={!descriptor()?.connection && !!descriptor()?.authMethods.length}>
      <SettingSelect label="登录方式" value={method() || descriptor()!.authMethods[0]}
        items={descriptor()?.authMethods.map(id => ({ value: id, title: ({
          qrcode: "二维码", sms: "短信", password: "账号密码", cookie: "Cookie", web: "网页登录",
        } as Record<string, string>)[id] })) ?? []}
        onChange={id => { void cancel(); method.set(id); cooldown.set(0); presentation.set({}); }} />
    </If>
    <If when={!!descriptor()?.connection || (method() || descriptor()?.authMethods[0]) === "password"}>
      <Each of={Object.entries(fields()).sort(([, a], [, b]) => Number(a.type === "secret") - Number(b.type === "secret"))} key={entry => entry[0]}>{entry => <Field
        label={entry()[1].label} value={values()[entry()[0]] || ""}
        placeholder={entry()[1].placeholder} secret={entry()[1].type === "secret"}
        onChange={value => values.set({ ...values(), [entry()[0]]: value })} />}</Each>
    </If>
    <If when={["cookie", "web"].includes(method() || descriptor()?.authMethods[0] || "")}>
      <If when={activeMethod() === "web"}>
        <Text class="text-sm text-muted-foreground">当前内嵌浏览器不支持独立会话和读取登录 Cookie，请在浏览器登录后粘贴 Cookie。</Text>
        <Text class="text-xs text-muted-foreground">{descriptor()?.auth?.web?.url}</Text>
        <Field label="登录完成后的 URL" value={pageUrl()} onChange={value => pageUrl.set(value)} />
      </If>
      <Field label="Cookie" value={cookie()} secret onChange={value => cookie.set(value)} />
    </If>
    <If when={activeMethod() === "sms"}>
      <Field label="手机号" value={phone()} onChange={value => phone.set(value)} />
      <Button label={cooldown() ? `${cooldown()} 秒后重试` : "发送验证码"}
        disabled={cooldown() > 0 || props.state.busy()} onPress={() => void step("sms.send", phone())
          .then(result => { if (result) cooldown.set(Number(result.result.cooldown ?? 60)); })} />
      <Field label="验证码" value={code()} onChange={value => code.set(value)} />
    </If>
    <If when={(method() || descriptor()?.authMethods[0]) === "qrcode"}>
      <SettingSelect label="扫码方式" value={variant() || Object.keys(descriptor()?.auth?.qrcode?.variants ?? {})[0] || ""} items={Object.entries(
        descriptor()?.auth?.qrcode?.variants ?? {}).map(([value, title]) => ({ value, title }))}
        onChange={value => variant.set(value)} />
      <If when={typeof presentation().image === "string"}>
        <Image testID="login-qr-image" src={String(presentation().image)} class="size-48 self-center"
          accessibilityLabel={qrExpired() ? "已过期的二维码" : "登录二维码"}
          style={{ opacity: qrExpired() ? 0.25 : 1 }} />
      </If>
      <Text class="text-sm text-muted-foreground">{qrStatus()}</Text>
      <Button label={qrExpired() ? "刷新二维码" : "生成或刷新二维码"} onPress={() => void startQr()} />
    </If>
    <If when={!!props.state.error()}><Text class="text-sm text-destructive">{props.state.error()}</Text></If>
    <View class="flex-row justify-end gap-2">
      <Button label="取消" onPress={props.onClose} />
      <If when={(method() || descriptor()?.authMethods[0]) !== "qrcode"}>
        <Button label={submitLabel()} variant="primary" loading={props.state.busy()} onPress={() => void submit()} />
      </If>
    </View>
  </View>;
}

/** Primary macOS-style import button opening the native menu on macOS and custom Menu elsewhere. */
function ImportPluginMenu(props: { disabled?: boolean; onPick: (kind: string) => void; onUrl: () => void }) {
  const nativeMenu = platform.os === "macos" && desktopAvailable;
  const triggerClass = `min-h-6 flex-row items-center justify-center rounded-(--radius-inset) bg-primary px-3 py-0.5 active:bg-primary/80 selected:bg-primary/80 disabled:opacity-40 ${focus}`;
  const label = () => <Text class="text-primary-foreground" style={{ fontSize: token("control-text-size") }}>导入…</Text>;
  const items = [
    { label: "ZIP 文件…", onSelect: () => props.onPick("zip") },
    { label: "本地目录…", onSelect: () => props.onPick("directory") },
    { label: "TypeScript 文件…", onSelect: () => props.onPick("typescript") },
    { label: "HTTPS URL…", onSelect: props.onUrl },
  ];
  if (nativeMenu) {
    const open = signal(false);
    let anchor: ViewRef | undefined;
    async function openMenu() {
      if (props.disabled || open()) return;
      open.set(true);
      try {
        const measured = await anchor?.measure();
        const at = measured
          ? { x: measured.pagePosition.x, y: measured.pagePosition.y + measured.bounds.size.height + 4 }
          : undefined;
        await DesktopMenu.popup(items, at);
      } finally {
        open.set(false);
      }
    }
    return <View ref={node => { anchor = node; }} class="shrink-0">
      <BaseButton testID="plugin-import-button" accessibilityLabel="导入插件" disabled={props.disabled}
        selected={open()} onPress={() => void openMenu()} class={triggerClass}>
        {label()}
      </BaseButton>
    </View>;
  }
  return <Menu trigger={
    <BaseButton testID="plugin-import-button" accessibilityLabel="导入插件" disabled={props.disabled}
      onPress={() => {}} class={triggerClass}>
      {label()}
    </BaseButton>
  }>
    <Each of={items} key={item => item.label}>{item =>
      <MenuItem onSelect={item().onSelect}><Text>{item().label}</Text></MenuItem>
    }</Each>
  </Menu>;
}

function connectionSummary(source: SourceRecord) {
  const address = source.connection?.baseUrl || "";
  let host = address;
  try { host = new URL(address.includes("://") ? address : `https://${address}`).host; } catch {}
  return [host, source.connection?.username].filter(Boolean).join(" · ") || "已连接";
}

export function PluginsSettings(props: { state: PluginsState; initialLogin?: string }) {
  const importing = signal(false), location = signal(""), removing = signal<string | null>(null);
  const loggingIn = signal(!!props.initialLogin);
  const loginPlugin = signal(props.initialLogin ?? "subsonic.music");
  const loginSource = signal<SourceRecord | undefined>(undefined);
  const loginLocked = signal(!!props.initialLogin);
  const uses = (id: string) => props.state.value().sources.filter(s => s.pluginId === id);
  const visiblePackages = () => props.state.value().packages.filter(p => p.kind !== "scraper");
  const canLogin = (pkg: PluginDescriptor) => !!pkg.connection || pkg.authMethods.length > 0;
  const sessions = (pkg: PluginDescriptor) => uses(pkg.id).filter(s =>
    pkg.kind === "library" ? !s.needsLogin && !!s.connection : !s.needsLogin || !!s.user
  );
  async function pick(kind: string) {
    await props.state.run(async c => {
      const path = await PluginService.pickImport(kind);
      if (path) await c.plugins.importPackage(path);
    });
  }
  async function openLogin(pkg: PluginDescriptor, source?: SourceRecord) {
    if (!pkg.enabled) {
      await props.state.run(c => c.plugins.setPackageEnabled(pkg.id, true));
    }
    loginSource.set(source);
    loginPlugin.set(pkg.id);
    loginLocked.set(true);
    loggingIn.set(true);
  }
  async function exportPlugin(pkg: PluginDescriptor) {
    await props.state.run(async c => {
      const path = await PluginService.pickExport(pkg.id);
      if (path) await c.plugins.exportPackage(pkg.id, path);
    });
  }
  async function toggleSource(source: SourceRecord, enabled: boolean) {
    await props.state.run(c => c.sources.setEnabled(source.sourceId, enabled));
  }
  async function logout(pkg: PluginDescriptor, source: SourceRecord) {
    await props.state.run(c =>
      pkg.kind === "library"
        ? c.sources.remove(source.sourceId)
        : c.sources.logout(source.sourceId)
    );
  }
  const removingPlugin = () => props.state.value().packages.find(p => p.id === removing());
  return <View class="flex-col gap-5" testID="plugins-settings">
    <SettingSection action={
      <ImportPluginMenu disabled={props.state.busy()} onPick={kind => void pick(kind)}
        onUrl={() => importing.set(true)} />
    }>
      <Each of={visiblePackages()} key={p => p.id}>{(p, index) => <>
        <If when={index() > 0}><SettingDivider /></If>
        <View class="flex-col">
          <View class="flex-row items-center justify-between gap-4 px-3 py-2">
            <Popover class="flex-1 min-w-0" popupClass={`${popupSurface} w-80 gap-3 p-4`}
              align="start" trigger={<BaseButton accessibilityLabel={`${p().name} 详情`}
                class={`w-full min-w-0 flex-row items-center gap-3 rounded-(--radius) py-1 active:bg-muted ${focus}`}>
                <View class="size-9 shrink-0 items-center justify-center rounded-(--radius) bg-card border border-border">
                  <If when={!!p().logo}><Image src={p().logo!} class="size-9 rounded-(--radius)" /></If>
                  <If when={!p().logo}><Icon name="plug" color="muted-foreground" /></If>
                </View>
                <View class="flex-1 min-w-0 gap-0.5">
                  <Text class="text-sm text-foreground">{p().name}</Text>
                  <Text class="text-xs text-muted-foreground">{[p().version, p().author, p().id].filter(Boolean).join(" · ")}</Text>
                </View>
              </BaseButton>}>
              <Text class="text-sm font-semibold text-foreground">{p().name}</Text>
              <Text class="text-xs text-muted-foreground">{[p().version, p().author, p().id].filter(Boolean).join(" · ")}</Text>
              <Text class="text-xs text-foreground">{p().capabilities.join("、")}</Text>
              <Text class="text-xs text-muted-foreground">{uses(p().id).length} 个实例{p().builtin ? " · 内置插件" : ""}</Text>
            </Popover>
            <ActionMenu
              label="…"
              accessibilityLabel={`${p().name} 操作`}
              items={[
                { label: "登录…", disabled: !canLogin(p()), onSelect: () => void openLogin(p()) },
                { label: "导出…", onSelect: () => void exportPlugin(p()) },
                { label: "删除…", separatorBefore: true, disabled: p().builtin, onSelect: () => removing.set(p().id) },
              ]}
            />
          </View>
          <If when={sessions(p()).length > 0}>
            <View class="flex-row flex-wrap items-center gap-2 px-3 pb-2.5">
              <Each of={sessions(p())} key={s => s.sourceId}>{s => {
                const isLib = () => p().kind === "library";
                const name = () => isLib()
                  ? connectionSummary(s())
                  : (s().user?.name || s().displayName || "已登录");
                const label = () => !s().enabled
                  ? `${name()} · 已禁用`
                  : s().needsLogin
                    ? `${name()} · 需重新登录`
                    : name();
                return <View testID={`plugin-session-${s().sourceId}`}
                  class="h-7 flex-row items-center gap-1.5 rounded-full border border-border bg-background pl-1 pr-1">
                  <View class={`size-5 shrink-0 items-center justify-center overflow-hidden rounded-full bg-muted ${!s().enabled ? "opacity-50" : ""}`}>
                    <If when={!!s().user?.avatar}>
                      <Image src={s().user!.avatar!} class="size-5 rounded-full" />
                    </If>
                    <If when={!s().user?.avatar}>
                      <Icon name={isLib() ? "radio" : "user"} class="size-3" color="muted-foreground" />
                    </If>
                  </View>
                  <Text
                    class={`max-w-52 px-0.5 text-xs ${!s().enabled ? "text-muted-foreground" : s().needsLogin ? "text-warning" : "text-foreground"}`}
                    style={{ numberOfLines: 1 }}
                  >
                    {label()}
                  </Text>
                  <ActionMenu
                    compact
                    label="…"
                    accessibilityLabel={`${name()} 菜单`}
                    items={[
                      ...(isLib() ? [{ label: "编辑连接…", onSelect: () => void openLogin(p(), s()) }] : []),
                      ...(!isLib() && !!s().needsLogin ? [{ label: "重新登录…", onSelect: () => void openLogin(p()) }] : []),
                      {
                        label: s().enabled ? "禁用" : "启用",
                        disabled: props.state.busy(),
                        onSelect: () => void toggleSource(s(), !s().enabled),
                      },
                      {
                        label: "登出",
                        separatorBefore: true,
                        disabled: props.state.busy(),
                        onSelect: () => void logout(p(), s()),
                      },
                    ]}
                  />
                </View>;
              }}</Each>
            </View>
          </If>
        </View>
      </>}</Each>
    </SettingSection>
    <Dialog open={loggingIn()} onOpenChange={e => loggingIn.set(e.open)} class="w-96 gap-4 p-5">
      <If when={loggingIn()}><SourceLogin state={props.state} initialPlugin={loginPlugin()}
        initialSource={loginSource()} lockedPlugin={loginLocked()}
        onClose={() => loggingIn.set(false)} /></If>
    </Dialog>
    <Dialog open={importing()} onOpenChange={e => importing.set(e.open)} class="w-96 gap-4 p-5">
      <Text role="header" class="text-lg font-semibold text-foreground">导入插件</Text>
      <Field label="文件、目录或 HTTPS URL" value={location()} onChange={value => location.set(value)} />
      <View class="flex-row justify-end gap-2"><Button label="取消" onPress={() => importing.set(false)} />
        <Button label="导入" variant="primary" loading={props.state.busy()} onPress={() => void props.state.run(async c => {
          await c.plugins.importPackage(location()); importing.set(false);
        })} /></View>
    </Dialog>
    <Dialog open={!!removing()} onOpenChange={e => { if (!e.open) removing.set(null); }} class="w-96 gap-4 p-5">
      <Text role="header" class="text-lg font-semibold text-foreground">删除插件？</Text>
      <Text class="text-sm text-muted-foreground">
        {uses(removing() ?? "").length > 0
          ? `将从本机移除 ${removingPlugin()?.name || removing()}，同时清除其登录信息与本地数据。`
          : `将从本机移除 ${removingPlugin()?.name || removing()}。`}
      </Text>
      <View class="flex-row justify-end gap-2"><Button label="取消" onPress={() => removing.set(null)} />
        <Button label="删除" variant="destructive" onPress={() => void props.state.run(async c => {
          const target = removing()!;
          for (const s of uses(target)) await c.sources.remove(s.sourceId);
          await c.plugins.deletePackage(target);
          removing.set(null);
        })} /></View>
    </Dialog>
  </View>;
}

/** Recursive data inspector; branches can be collapsed without losing the result. */
function JsonTree(props: { value: unknown; name?: string }) {
  const expanded = signal(true);
  const entries = () => props.value && typeof props.value === "object" && Object.keys(props.value).length
    ? Object.entries(props.value) : null;
  return <View class="gap-1 items-start">
    <If when={!!entries()}>
      <BaseButton accessibilityLabel={`${expanded() ? "收起" : "展开"}${props.name || "结果"}`}
        class={`min-h-7 flex-row items-center gap-1 rounded-(--radius) ${focus}`}
        onPress={() => expanded.set(!expanded())}>
        <Text class="w-4 text-xs text-muted-foreground">{expanded() ? "▾" : "▸"}</Text>
        <Text class="text-xs text-foreground">{props.name || "结果"}</Text>
      </BaseButton>
      <If when={expanded()}><View class="gap-1 ps-4 items-start">
        <Each of={entries() || []} key={e => e[0]}>{entry => <JsonTree
          name={entry()[0]} value={entry()[1]} />}</Each>
      </View></If>
    </If>
    <If when={!entries()}><View class="min-h-7 flex-row items-center gap-1">
      <View class="w-4 shrink-0" />
      <Text class="text-xs text-foreground">{props.name}: {JSON.stringify(props.value)}</Text>
    </View></If>
  </View>;
}

function eventLine(value: unknown) {
  const event = value as { kind?: string; method?: string; phase?: string;
    status?: number; url?: string; level?: string; message?: string };
  return event.kind === "http"
    ? `HTTP ${event.method} · ${event.phase}${event.status ? ` · ${event.status}` : ""}\n${event.url}`
    : `${event.level || "日志"} · ${event.message || JSON.stringify(value)}`;
}

export function DeveloperSettings(props: { state: PluginsState; initialResult?: unknown }) {
  const source = signal(""), capability = signal("track.get"), args = signal("[]");
  const raw = signal(false), values = signal<Record<string, string>>({});
  const watching = signal(""), watchStatus = signal("");
  let stopWatch: (() => void) | undefined;
  cleanup(() => stopWatch?.());
  const location = signal(""), result = signal<unknown>(props.initialResult ?? null);
  const activeSource = () => source() || props.state.value().sources[0]?.sourceId || "";
  const plugin = () => props.state.value().sources.find(s => s.sourceId === activeSource())?.pluginId;
  const descriptor = () => props.state.value().packages.find(p => p.id === plugin());
  const fields = () => descriptor()?.parameters?.[capability()] || [];
  effect(() => {
    const path = capability();
    const parameters = descriptor()?.parameters?.[path] || [];
    values.set(Object.fromEntries(parameters.map(field => [field.key, field.default])));
  });
  function argumentsForCall() {
    if (raw()) return { args: JSON.parse(args()), context: { limit: 20 } };
    const argsValue: unknown[] = [], context: Record<string, unknown> = {};
    for (const field of fields()) {
      const input = values()[field.key] ?? field.default;
      if (input === "" && field.key === "cursor") continue;
      const value = field.type === "number" ? Number(input)
        : field.type === "json" ? JSON.parse(input) : input;
      if (field.target === "args") return { args: value as unknown[], context };
      if (field.target === "context") context[field.key] = value;
      else if (field.target === "arg") argsValue[field.index || 0] = value;
      else {
        const index = field.index || 0;
        argsValue[index] = { ...(argsValue[index] as object || {}), [field.key]: value };
      }
    }
    return { args: argsValue, context };
  }
  async function load() {
    stopWatch?.(); watching.set("");
    await props.state.run(async c => {
      await c.plugins.importPackage(location());
      stopWatch = await c.plugins.watchDirectory(location(), status => watchStatus.set(status));
      watching.set(location()); watchStatus.set("正在监听文件变化");
    });
  }
  return <View class="flex-row items-start gap-5 min-w-0" testID="developer-settings">
    <View class="flex-1 min-w-0 flex-col gap-5">
      <SettingSection title="本地插件调试">
        <SettingCardBlock>
          <Field label="本地插件目录" value={location()} onChange={value => location.set(value)} />
          <View class="flex-row"><Button label="加载或重新加载" size="sm" loading={props.state.busy()}
            onPress={() => void load()} /></View>
          <If when={!!watching()}><View class="flex-row items-center gap-2">
            <Text class="flex-1 text-xs text-muted-foreground">{watchStatus()}：{watching()}</Text>
            <Button label="停止监听" size="sm" onPress={() => {
              stopWatch?.(); watching.set(""); watchStatus.set("");
            }} />
          </View></If>
        </SettingCardBlock>
      </SettingSection>
      <SettingSection title="能力调用">
        <SettingSelectRow label="实例" value={activeSource()} items={props.state.value().sources.map(s => ({
          value: s.sourceId, title: s.displayName || s.sourceId,
        }))} onChange={value => source.set(value)} />
        <SettingSelectRow divider label="能力" value={capability()} items={descriptor()?.capabilities.map(value => ({
          value, title: value,
        })) ?? []} onChange={value => capability.set(value)} />
        <SettingSwitchRow divider label="原始 JSON" checked={raw()} onChange={value => raw.set(value)} />
        <SettingCardBlock divider>
          <If when={raw()}><Field label="参数（JSON 数组）" value={args()}
            onChange={value => args.set(value)} /></If>
          <If when={!raw()}><View class="gap-3" testID="plugin-parameter-form">
            <Each of={fields()} key={field => field.key}>{field => <Field label={field().label}
              value={values()[field().key] ?? field().default}
              onChange={value => values.set({ ...values(), [field().key]: value })} />}</Each>
            <If when={fields().length === 0}><Text class="text-xs text-muted-foreground">此能力无需参数。</Text></If>
          </View></If>
          <View class="flex-row flex-wrap gap-2">
            <Button label="调用" variant="primary" size="sm" disabled={!activeSource()} loading={props.state.busy()}
              onPress={() => void props.state.run(async c => {
                const input = argumentsForCall();
                result.set(await c.plugins.invoke(activeSource(), capability(), input.args, input.context));
              })} />
            <Button label="运行契约测试" size="sm" disabled={!plugin()} loading={props.state.busy()}
              onPress={() => void props.state.run(async c => result.set(await c.plugins.testContract(plugin()!)))} />
            <Button label="查看存储" size="sm" disabled={!activeSource()}
              onPress={() => void props.state.run(async c => result.set(await c.plugins.storage(activeSource())))} />
            <Button label="清除 KV" size="sm" variant="destructive" disabled={!activeSource()}
              onPress={() => void props.state.run(c => c.plugins.clearStorage(activeSource()))} />
            <Button label="清除凭据" size="sm" variant="destructive" disabled={!activeSource()}
              onPress={() => void props.state.run(c => c.sources.logout(activeSource()))} />
          </View>
        </SettingCardBlock>
      </SettingSection>
    </View>
    <View class="flex-1 min-w-0 flex-col gap-5">
      <If when={result() !== null}>
        <SettingSection title="返回结果" testID="plugin-call-result">
          <SettingCardBlock>
            <Scroll class="h-64 min-h-0" contentClass="items-start pb-2" testID="plugin-result-scroll">
              <JsonTree value={result()} />
            </Scroll>
          </SettingCardBlock>
        </SettingSection>
      </If>
      <SettingSection title="日志与 HTTP 追踪">
        <SettingCardBlock>
          <List each={props.state.events()} class="h-48 min-h-0" testID="plugin-events"
            keyExtractor={(_, index) => String(index)}
            empty={<Text class="text-xs text-muted-foreground">尚无日志或 HTTP 追踪。调用能力后显示。</Text>}>
            {event => <View class="border-b border-settings-divider py-2">
              <Text class="text-xs text-muted-foreground" style={{ numberOfLines: 3 }}>{eventLine(event())}</Text>
            </View>}
          </List>
        </SettingCardBlock>
      </SettingSection>
    </View>
  </View>;
}
