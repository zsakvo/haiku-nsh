import { nativeComponent, signal, Text, View } from "nsh";
import { Button } from "../ui/Button";

const GlassSurface = nativeComponent("haiku.GlassSurface", { props: [] });

/** Native background, with NSH text and controls in the next sibling. */
export function GlassStrip(props: { onHeight?: (height: number) => void }) {
  const taps = signal(0);
  return (
    <View testID="glass-strip" class="relative h-16 shrink-0"
      onLayout={(event) => props.onHeight?.(event.bounds.size.height)}>
      <GlassSurface class="absolute inset-0" />
      <View class="flex-1 flex-row items-center justify-between px-4">
        <Text class="text-base text-foreground">玻璃底栏 · NSH 内容 {taps()}</Text>
        <Button label="底栏按钮" onPress={() => taps.set(taps() + 1)} />
      </View>
    </View>
  );
}
