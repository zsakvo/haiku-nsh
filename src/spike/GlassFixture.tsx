import { mount, signal, Text, View } from "nsh";
import { GlassStrip } from "./Glass";

function Fixture() {
  const height = signal(0);
  return (
    <View class="flex-1">
      <Text class="flex-1">Headless 底栏高度 {height()}</Text>
      <GlassStrip onHeight={(value) => height.set(value)} />
    </View>
  );
}
mount(Fixture);
