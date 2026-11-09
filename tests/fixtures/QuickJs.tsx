import { cleanup, mount, signal, Text, View } from "nsh";
import { Spike } from "haiku/native";
import { Button } from "../../src/ui/Button";

// The workload runs inside the app's NSH VM, alongside its native calls.
function QuickJsProbe() {
  const status = signal("Ready");
  let timer: number | undefined;
  cleanup(() => clearInterval(timer));

  async function run() {
    status.set("Running");
    let completed = 0;
    let pending = 0;
    let ticks = 0;
    let overlapTicks = 0;
    let checksum = 0;
    const globals = globalThis as typeof globalThis & {
      __haikuUiLeak?: number;
      __haikuPluginLeak?: number;
    };
    globals.__haikuUiLeak = 73;
    timer = setInterval(() => {
      // Allocate, sort, and calculate while plugin VM threads are executing.
      const values = Array.from({ length: 128 }, (_, i) => (i * 17 + ticks) % 251);
      values.sort((a, b) => a - b);
      checksum += values.reduce((sum, value) => sum + value * value, 0);
      ticks++;
      if (pending > 0) overlapTicks++;
    }, 1);
    try {
      // 16 in-flight calls; 1000 independently created/loaded plugin runtimes.
      await Promise.all(Array.from({ length: 16 }, async (_, worker) => {
        for (let n = worker; n < 1000; n += 16) {
          pending++;
          try {
            const result = await Spike.pluginEcho(n);
            if (result !== n * 2) throw new Error(`echo(${n}): ${result}`);
            if (typeof globals.__haikuPluginLeak !== "undefined" || globals.__haikuUiLeak !== 73) {
              throw new Error("Plugin global leaked into NSH UI VM");
            }
            completed++;
          } finally {
            pending--;
          }
        }
      }));
      if (completed !== 1000 || overlapTicks === 0 || !Number.isFinite(checksum) || checksum <= 0) {
        throw new Error(`completed=${completed}, overlap=${overlapTicks}, checksum=${checksum}`);
      }
      console.log(`QuickJS coexistence: calls=${completed}, ticks=${ticks}, overlap=${overlapTicks}, checksum=${checksum}, isolation=both-directions`);
      status.set("Passed 1000 echoes with overlapping JS computation");
    } catch (error) {
      status.set(`Failed: ${String(error)}`);
    } finally {
      clearInterval(timer);
      delete globals.__haikuUiLeak;
    }
  }

  return (
    <View class="flex-1 gap-4 bg-background p-8">
      <Button label="Run 1000 echoes" onPress={run} />
      <Text class="text-foreground">{status()}</Text>
    </View>
  );
}

mount(QuickJsProbe);
