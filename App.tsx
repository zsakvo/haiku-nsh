import { mount, signal, Text, View } from "nsh";
import { Core, Spike } from "haiku/native";
import { Button } from "./src/ui/Button";
import { PlatformSpikes } from "./src/spike/PlatformSpikes";

function App() {
  const pending = signal(false);
  const result = signal("等待验证");
  async function runEcho() {
    if (pending()) return;
    pending.set(true);
    result.set("验证中");
    try {
      result.set(`Plugin echo(21) = ${await Spike.pluginEcho(21)}`);
    } catch (error) {
      result.set(`验证失败：${String(error)}`);
    } finally {
      pending.set(false);
    }
  }
  return (
    <View class="flex-1 gap-4 bg-background p-6">
      <View class="flex-row items-center gap-4">
        <Text class="text-xl font-semibold text-foreground">Haiku</Text>
        <Text class="text-sm text-muted-foreground">Core {Core.version()}</Text>
        <Button label="验证插件回声" disabled={pending()} onPress={runEcho} />
        <Text class="text-base text-foreground">{result()}</Text>
      </View>
      <PlatformSpikes />
    </View>
  );
}

mount(App);
