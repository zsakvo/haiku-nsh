import { mount, signal, Text, View } from "nsh";
import { SeekBar } from "haiku/native";
import { Button } from "./src/ui/Button";

function Fixture() {
  const position = signal(5);
  const duration = signal(100);
  const commits = signal(0);
  const last = signal(-1);
  return <View class="flex-1 p-6 gap-4">
    <View style={{ width: 400 }}>
      <SeekBar position={position()} duration={duration()} buffered={0.8}
        onSeek={(event) => { commits.set(commits() + 1); last.set(event.seconds); position.set(event.seconds); }} />
    </View>
    <Text>{`commits:${commits()};last:${last().toFixed(3)}`}</Text>
    <Button label="推进行钟" onPress={() => setTimeout(() => position.set(90), 200)} />
    <Button label="未知时长" onPress={() => duration.set(0)} />
  </View>;
}
mount(Fixture);
