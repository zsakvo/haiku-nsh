import { cleanup, effect, Image, Scroll, signal, Text, View } from "nsh";
import { Button as BaseButton, Popover } from "nsh/base";
import { Button, focus } from "../ui/Button";
import { Icon } from "../ui/Icon";
import { Field } from "../ui/LibraryControls";
import { Dialog, Menu, MenuItem, popupSurface } from "../ui/Surfaces";
import { SettingSelect, SettingSwitch } from "../ui/SettingsControls";
import type { PluginsState, SourceRecord } from "../state/plugins";

export function SourcesSettings(props: { state: PluginsState; initialLogin?: string }) {
  const adding = signal(!!props.initialLogin);
  const plugin = signal(props.initialLogin ?? "subsonic.music");
  const editing = signal<string | null>(null), name = signal("");
  const removing = signal<string | null>(null);
  const connection = signal<SourceRecord | undefined>(undefined);
  const isLibrary = (id: string) => props.state.value().packages.find(p => p.id === id)?.kind === "library";
  function subtitle(source: SourceRecord) {
    if (!isLibrary(source.pluginId)) return source.needsLogin ? "需要重新登录" : source.user?.name || "未登录";
    const address = source.connection?.baseUrl || "";
    let host = address;
    try { host = new URL(address.includes("://") ? address : `https://${address}`).host; } catch {}
    return [host, source.connection?.username].filter(Boolean).join(" · ") || "尚未配置连接";
  }
  async function move(id: string, delta: number) {
    const ids = props.state.value().sources.map(s => s.sourceId);
    const from = ids.indexOf(id), to = from + delta;
    if (to < 0 || to >= ids.length) return;
    [ids[from], ids[to]] = [ids[to], ids[from]];
    await props.state.run(c => c.sources.reorder(ids));
  }
  return <View class="gap-4" testID="sources-settings">
    <Text role="header" class="text-lg font-semibold text-foreground">来源</Text>
    <Text class="text-sm text-muted-foreground">连接音乐库或登录音乐平台，管理账号与来源顺序。</Text>
    <Each of={props.state.value().sources} key={s => s.sourceId}>{s => <View
      class="flex-row items-center gap-4 border-b border-border py-3">
      <View class="flex-1 min-w-0 gap-1">
        <Text class="text-sm font-medium text-foreground">{s().displayName || s().pluginId}</Text>
        <Text class="text-xs text-muted-foreground">{subtitle(s())}</Text>
      </View>
      <View class="shrink-0 items-end gap-2">
        <SettingSwitch label="启用来源" hideLabel checked={s().enabled}
          onChange={enabled => void props.state.run(c => c.sources.setEnabled(s().sourceId, enabled))} />
        <View class="flex-row items-center justify-end gap-2">
          <If when={isLibrary(s().pluginId)}>
            <Button label="编辑连接…" size="sm" onPress={() => {
              connection.set(s()); plugin.set(s().pluginId); adding.set(true);
            }} />
          </If>
          <If when={!isLibrary(s().pluginId)}>
            <Button label="登录…" size="sm" onPress={() => {
              connection.set(undefined); plugin.set(s().pluginId); adding.set(true);
            }} />
            <Button label="登出" size="sm" disabled={props.state.busy()}
              onPress={() => void props.state.run(c => c.sources.logout(s().sourceId))} />
          </If>
          <Menu trigger={<Button label="更多…" size="sm" onPress={() => {}} />}>
            <MenuItem onSelect={() => { editing.set(s().sourceId); name.set(s().displayName || s().pluginId); }}><Text>重命名…</Text></MenuItem>
            <MenuItem onSelect={() => void move(s().sourceId, -1)}><Text>上移</Text></MenuItem>
            <MenuItem onSelect={() => void move(s().sourceId, 1)}><Text>下移</Text></MenuItem>
            <MenuItem onSelect={() => removing.set(s().sourceId)}><Text>移除…</Text></MenuItem>
          </Menu>
        </View>
      </View>
    </View>}</Each>
    <If when={props.state.value().sources.length === 0}>
      <Text class="text-sm text-muted-foreground">尚未添加来源。先连接一个音乐库。</Text>
    </If>
    <View class="flex-row"><Button label="添加新来源…" onPress={() => { connection.set(undefined); adding.set(true); }} /></View>
    <Dialog open={adding()} onOpenChange={e => adding.set(e.open)} class="w-96 max-w-full self-center mt-24 gap-4 p-5">
      <If when={adding()}><SourceLogin state={props.state} initialPlugin={plugin()} initialSource={connection()}
        onClose={() => adding.set(false)} /></If>
    </Dialog>
    <Dialog open={!!editing()} onOpenChange={e => { if (!e.open) editing.set(null); }} class="w-96 max-w-full self-center mt-24 gap-4 p-5">
      <Text role="header" class="text-lg font-semibold text-foreground">重命名来源</Text>
      <Field label="来源名称" value={name()} onChange={value => name.set(value)} />
      <Button label="保存" onPress={() => void props.state.run(async c => {
        await c.sources.rename(editing()!, name()); editing.set(null);
      })} />
    </Dialog>
    <Dialog open={!!removing()} onOpenChange={e => { if (!e.open) removing.set(null); }} class="w-96 max-w-full self-center mt-24 gap-4 p-5">
      <Text role="header" class="text-lg font-semibold text-foreground">移除来源？</Text>
      <Text class="text-sm text-muted-foreground">此来源的凭据和插件存储将被删除。</Text>
      <View class="flex-row justify-end gap-2">
        <Button label="取消" onPress={() => removing.set(null)} />
        <Button label="移除" variant="destructive" onPress={() => void props.state.run(async c => {
          await c.sources.remove(removing()!); removing.set(null);
        })} />
      </View>
    </Dialog>
  </View>;
}

export function SourceLogin(props: { state: PluginsState; initialPlugin: string; initialSource?: SourceRecord; onClose: () => void }) {
  const plugin = signal(props.initialPlugin), method = signal(""), values = signal<Record<string, string>>(Object.fromEntries(
    Object.entries(props.initialSource?.connection ?? {}).filter(([, value]) => value !== "[REDACTED]")
  ));
  let session: string | null = null;
  const presentation = signal<Record<string, unknown>>({});
  const phone = signal(""), code = signal(""), cookie = signal(""), pageUrl = signal("");
  const cooldown = signal(0), variant = signal(""), qrStatus = signal("");
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
    qrStatus.set("正在生成二维码…");
    const start = await step("qrcode.start", variant());
    if (!start || closed || revision !== generation) return;
    presentation.set(start.result); qrStatus.set("请使用手机扫码");
    while (!closed && revision === generation && session) {
      const poll = await step("qrcode.poll", null);
      if (!poll) { qrStatus.set("登录中断，请重试"); break; }
      if (poll.done) break;
      const status = String(poll.result.status);
      qrStatus.set(status === "scanned" ? "已扫描，请在手机上确认"
        : status === "expired" ? "二维码已过期，请刷新" : "请使用手机扫码");
      if (status === "expired") break;
    }
  }
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
  return <View class="gap-4" testID="source-login">
    <Text role="header" class="text-lg font-semibold text-foreground">{props.initialSource ? "编辑连接" : "添加新来源"}</Text>
    <If when={!props.initialSource}><SettingSelect label="插件" value={plugin()} items={props.state.value().packages
      .filter(p => p.kind !== "scraper").map(p => ({ value: p.id, title: p.name }))}
      onChange={id => { void cancel(); plugin.set(id); values.set({}); method.set(""); }} /></If>
    <If when={!descriptor()?.connection && !!descriptor()?.authMethods.length}>
      <SettingSelect label="登录方式" value={method() || descriptor()!.authMethods[0]}
        items={descriptor()?.authMethods.map(id => ({ value: id, title: ({
          qrcode: "二维码", sms: "短信", password: "账号密码", cookie: "Cookie", web: "网页登录",
        } as Record<string, string>)[id] })) ?? []}
        onChange={id => { void cancel(); method.set(id); }} />
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
      <SettingSelect label="扫码方式" value={variant()} items={Object.entries(
        descriptor()?.auth?.qrcode?.variants ?? {}).map(([value, title]) => ({ value, title }))}
        onChange={value => { variant.set(value); void cancel(); }} />
      <If when={typeof presentation().image === "string"}>
        <Image src={String(presentation().image)} class="size-48 self-center" />
      </If>
      <Text class="text-sm text-muted-foreground">{qrStatus()}</Text>
      <Button label="生成或刷新二维码" onPress={() => void startQr()} />
    </If>
    <If when={!!props.state.error()}><Text class="text-sm text-destructive">{props.state.error()}</Text></If>
    <View class="flex-row justify-end gap-2">
      <Button label="取消" onPress={props.onClose} />
      <If when={(method() || descriptor()?.authMethods[0]) !== "qrcode"}>
        <Button label={props.initialSource ? "验证并保存" : "连接并添加"} variant="primary" loading={props.state.busy()} onPress={() => void submit()} />
      </If>
    </View>
  </View>;
}

export function PluginsSettings(props: { state: PluginsState }) {
  const importing = signal(false), location = signal(""), removing = signal<string | null>(null);
  const uses = (id: string) => props.state.value().sources.filter(s => s.pluginId === id);
  return <View class="gap-4" testID="plugins-settings">
    <View class="flex-row items-center justify-between">
      <Text role="header" class="text-lg font-semibold text-foreground">插件</Text>
      <Menu trigger={<Button label="导入…" onPress={() => {}} />}>
        {["ZIP 文件…", "本地目录…", "TypeScript 文件…", "HTTPS URL…"].map(label =>
          <MenuItem onSelect={() => importing.set(true)}><Text>{label}</Text></MenuItem>)}
      </Menu>
    </View>
    <Each of={props.state.value().packages} key={p => p.id}>{p => <View
      class="flex-row items-center justify-between gap-4 border-b border-border py-3">
      <Popover class="flex-1 min-w-0" popupClass={`${popupSurface} w-80 gap-3 p-4`}
        align="start" trigger={<BaseButton accessibilityLabel={`${p().name} 详情`}
          class={`w-full min-w-0 flex-row items-center gap-3 rounded-(--radius) p-2 hover:bg-accent active:bg-muted ${focus}`}>
          <View class="size-9 shrink-0 items-center justify-center rounded-(--radius) bg-muted">
            <If when={!!p().logo}><Image src={p().logo!} class="size-9 rounded-(--radius)" /></If>
            <If when={!p().logo}><Icon name="plug" color="muted-foreground" /></If>
          </View>
          <View class="flex-1 min-w-0 gap-1">
            <Text class="text-sm font-medium text-foreground">{p().name}</Text>
            <Text class="text-xs text-muted-foreground">{p().version} · {p().author || "未提供作者"} · {p().id}</Text>
          </View>
        </BaseButton>}>
        <Text class="text-sm font-semibold text-foreground">{p().name}</Text>
        <Text class="text-xs text-muted-foreground">{p().version} · {p().author || "未提供作者"} · {p().id}</Text>
        <Text class="text-xs text-foreground">{p().capabilities.join("、")}</Text>
        <Text class="text-xs text-muted-foreground">{uses(p().id).length} 个实例{p().builtin ? " · 内置插件" : ""}</Text>
        <Button label="删除插件…" variant="destructive"
          disabled={p().builtin || uses(p().id).length > 0} onPress={() => removing.set(p().id)} />
      </Popover>
      <View class="w-9 shrink-0 items-end">
        <SettingSwitch label={`启用${p().name}`} hideLabel checked={p().enabled}
          onChange={enabled => void props.state.run(c => c.plugins.setPackageEnabled(p().id, enabled))} />
      </View>
    </View>}</Each>
    <Dialog open={importing()} onOpenChange={e => importing.set(e.open)} class="w-96 max-w-full self-center mt-24 gap-4 p-5">
      <Text role="header" class="text-lg font-semibold text-foreground">导入插件</Text>
      <Field label="文件、目录或 HTTPS URL" value={location()} onChange={value => location.set(value)} />
      <View class="flex-row justify-end gap-2"><Button label="取消" onPress={() => importing.set(false)} />
        <Button label="导入" variant="primary" loading={props.state.busy()} onPress={() => void props.state.run(async c => {
          await c.plugins.importPackage(location()); importing.set(false);
        })} /></View>
    </Dialog>
    <Dialog open={!!removing()} onOpenChange={e => { if (!e.open) removing.set(null); }} class="w-96 max-w-full self-center mt-24 gap-4 p-5">
      <Text role="header" class="text-lg font-semibold text-foreground">删除插件？</Text>
      <Text class="text-sm text-muted-foreground">将从本机移除 {removing()}。</Text>
      <View class="flex-row justify-end gap-2"><Button label="取消" onPress={() => removing.set(null)} />
        <Button label="删除" variant="destructive" onPress={() => void props.state.run(async c => {
          await c.plugins.deletePackage(removing()!); removing.set(null);
        })} /></View>
    </Dialog>
  </View>;
}

/** Recursive data inspector; branches can be collapsed without losing the result. */
function JsonTree(props: { value: unknown; name?: string }) {
  const expanded = signal(true);
  const entries = () => props.value && typeof props.value === "object" ? Object.entries(props.value) : null;
  return <View class="gap-1 items-start">
    <If when={!!entries()}>
      <BaseButton accessibilityLabel={`${expanded() ? "收起" : "展开"}${props.name || "结果"}`}
        class={`min-h-7 flex-row items-center gap-1 rounded-(--radius) hover:bg-accent ${focus}`}
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

export function DeveloperSettings(props: { state: PluginsState; initialResult?: unknown }) {
  const source = signal(""), capability = signal("track.get"), args = signal('["song:1/字"]');
  const location = signal(""), result = signal<unknown>(props.initialResult ?? null);
  const activeSource = () => source() || props.state.value().sources[0]?.sourceId || "";
  const plugin = () => props.state.value().sources.find(s => s.sourceId === activeSource())?.pluginId;
  const descriptor = () => props.state.value().packages.find(p => p.id === plugin());
  return <View class="gap-4" testID="developer-settings">
    <Text role="header" class="text-lg font-semibold text-foreground">开发者</Text>
    <Field label="本地插件目录" value={location()} onChange={value => location.set(value)} />
    <View class="flex-row"><Button label="加载或重新加载" loading={props.state.busy()}
      onPress={() => void props.state.run(c => c.plugins.importPackage(location()))} /></View>
    <SettingSelect label="实例" value={activeSource()} items={props.state.value().sources.map(s => ({
      value: s.sourceId, title: s.displayName || s.sourceId,
    }))} onChange={value => source.set(value)} />
    <SettingSelect label="能力" value={capability()} items={descriptor()?.capabilities.map(value => ({
      value, title: value,
    })) ?? []} onChange={value => capability.set(value)} />
    <Field label="参数（JSON 数组）" value={args()} onChange={value => args.set(value)} />
    <View class="flex-row gap-2">
      <Button label="调用" variant="primary" disabled={!activeSource()} loading={props.state.busy()}
        onPress={() => void props.state.run(async c => result.set(await c.plugins.invoke(
          activeSource(), capability(), JSON.parse(args()), { limit: 20 },
        )))} />
      <Button label="运行契约测试" disabled={!plugin()} loading={props.state.busy()}
        onPress={() => void props.state.run(async c => result.set(await c.plugins.testContract(plugin()!)))} />
      <Button label="查看存储" disabled={!activeSource()}
        onPress={() => void props.state.run(async c => result.set(await c.plugins.storage(activeSource())))} />
    </View>
    <View class="flex-row gap-2">
      <Button label="清除 KV" disabled={!activeSource()}
        onPress={() => void props.state.run(c => c.plugins.clearStorage(activeSource()))} />
      <Button label="清除凭据" disabled={!activeSource()}
        onPress={() => void props.state.run(c => c.sources.logout(activeSource()))} />
    </View>
    <If when={result() !== null}><View class="gap-2" testID="plugin-call-result">
      <Text class="text-sm font-medium text-foreground">返回结果</Text>
      <Scroll class="h-64 min-h-0" contentClass="items-start pb-2" testID="plugin-result-scroll">
        <JsonTree value={result()} />
      </Scroll>
    </View></If>
    <Text role="header" class="text-sm font-medium text-foreground">日志与 HTTP 追踪</Text>
    <Each of={props.state.events()}>{event => <Text class="text-xs text-muted-foreground">{JSON.stringify(event())}</Text>}</Each>
  </View>;
}
