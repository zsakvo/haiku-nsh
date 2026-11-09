import { cleanup, effect, signal, Text, View } from "nsh";
import { Spike, SpikeCounter, SpikeTrackList } from "haiku/native";
import { Button } from "../ui/Button";

/** Disposable S5/S6/S8 surface, sharing the existing app button and theme. */
export function PlatformSpikes() {
  const counter = new SpikeCounter();
  const count = signal(counter.value);
  counter.onChange((value) => count.set(value));
  cleanup(() => counter.dispose());
  const path = Spike.audioPath();
  const playback = signal(path ? "准备本地音频" : "未设置 HAIKU_SPIKE_AUDIO");
  const position = signal(0);
  async function play() {
    if (!path) return;
    try {
      await Spike.playLocal(path);
      playback.set("播放中");
    } catch (error) {
      playback.set(`播放失败：${String(error)}`);
      console.error(`S5 ${String(error)}`);
    }
  }
  effect(() => {
    if (path) void play();
    const timer = setInterval(() => {
      const seconds = Spike.position();
      position.set(seconds);
      if (path) console.log(`S5 position()=${seconds.toFixed(3)}`);
    }, 1000);
    return () => clearInterval(timer);
  });
  return (
    <View class="flex-1 gap-3">
      <View class="flex-row items-center gap-3">
        <Text class="text-base text-foreground">Rust 计数器 {count()}</Text>
        <Button label="+1" onPress={() => counter.increment()} />
        <Button label="打开设置窗口" disabled onPress={() => {}} />
        <Text class="text-sm text-muted-foreground">S8：当前 NSH 无第二窗口 API</Text>
      </View>
      <View class="flex-row items-center gap-3">
        <Button label="播放本地音频" disabled={!path} onPress={() => void play()} />
        <Button label="暂停" onPress={() => Spike.pause()} />
        <Text class="text-sm text-foreground">{playback()} · {position().toFixed(1)} 秒</Text>
      </View>
      <Text class="text-sm text-muted-foreground">
        S5：当前 NSH 无关闭拦截与重新激活钩子；S6：10000 行，选中状态按曲目 ID 保存
      </Text>
      <SpikeTrackList class="flex-1" />
    </View>
  );
}
