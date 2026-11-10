import { cleanup, mount, resource, signal, Text, View } from 'nsh';
import { Core } from 'haiku/native';
import { createPlugins } from './src/state/plugins';
import { SourceLogin } from './src/shell/PluginSettings';
import { Button } from './src/ui/Button';
import { Dialog } from './src/ui/Surfaces';
import { createAppearance } from './src/ui/theme';

function Fixture() {
  createAppearance('light');
  const ready = resource(async () => {
    const c = await Core.openIsolated();
    await c.plugins.importPackage('native/app/tests/fixtures/auth-plugin');
    return c;
  });
  // Child stores may call Core.open(); select the isolated root before creating them.
  return <View class="flex-1">
    <If when={!!ready()}><AuthPanel core={ready()!} /></If>
    <If when={!!ready.error()}><Text>{String(ready.error())}</Text></If>
  </View>;
}
function AuthPanel(props: { core: Core }) {
  const state = createPlugins(), method = signal('qrcode'), opened = signal(true);
  const report = signal(''), events = signal(0);
  const stop = props.core.plugins.onEvent(() => events.set(events() + 1));
  cleanup(stop);
  return <View class="flex-1 gap-4 p-4 bg-background">
      <View class="flex-row gap-2">
        {['qrcode', 'sms', 'cookie', 'web'].map(value => <Button label={`打开 ${value}`}
          onPress={() => { method.set(value); opened.set(true); }} />)}
        <Button label="读取来源" onPress={() => {
          report.set("reading");
          void props.core.sources.list().then(value => {
            report.set(`sources:${(value as { sources: unknown[] }).sources.length}`);
          });
        }} />
        <Button label="停止事件订阅" onPress={() => { stop?.(); report.set(`stopped:${events()}`); }} />
        <Button label="触发事件" onPress={() => void props.core.plugins.invoke(
          'auth.fixture', 'track.get', ['fixture'], {},
        )} />
      </View>
      <Text>{report()}</Text><Text>{`events:${events()}`}</Text>
      <Each of={state.events()}>{event => <Text>{JSON.stringify(event())}</Text>}</Each>
      <Dialog open={opened()} onOpenChange={e => opened.set(e.open)}
        class="w-96 self-center mt-12 gap-4 p-5">
        <If when={opened()}><Each of={[method()]} key={value => value}>{value =>
          <SourceLogin state={state} initialPlugin="auth.fixture" initialMethod={value()}
            onClose={() => opened.set(false)} />
        }</Each></If>
      </Dialog>
  </View>;
}
mount(Fixture);
