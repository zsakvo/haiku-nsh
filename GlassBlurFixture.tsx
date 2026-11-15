import { mount, nativeComponent, resource, Text, View } from "nsh";
import { Core } from "haiku/native";
import theme from "./src/ui/theme/haiku.json";

// Inspect with a WindowServer capture: app.screenshot() uses cacheDisplayInRect,
// which omits the system's final backdrop composition.
const Glass = nativeComponent<{ radius: number; dark: boolean }>(
  "haiku.GlassSurface", { props: ["radius", "dark"] },
);

function Fixture() {
  const ready = resource(async () => {
    await Core.applyTheme(JSON.stringify(theme));
    return true;
  });
  return <View class="flex-1 bg-background p-6 gap-4">
    <Text class="text-xl text-foreground">Background blur comparison</Text>
    <If when={ready()}>
      <View class="flex-1 flex-row gap-6">
        {[false, true].map(blurred => <View class="flex-1 gap-3">
          <Text class="text-sm text-foreground">{blurred ? "Native backdrop blur" : "Original"}</Text>
          <View class="flex-1 overflow-hidden">
            <View class="absolute inset-0 flex-row">
              {Array.from({length:40},(_,i)=><View class={`flex-1 ${i%2 ? "bg-background" : "bg-foreground"}`} />)}
            </View>
            <View class="absolute top-12 left-0 right-0 h-16 bg-brand-blue" />
            <View class="absolute top-36 left-0 right-0 h-16 bg-brand-orange" />
            <Text class="absolute top-24 left-4 text-3xl text-foreground">BLUR TEST 123</Text>
            <If when={blurred}><Glass testID="blur-probe" class="absolute inset-0"
              radius={0} dark={false} /></If>
          </View>
        </View>)}
      </View>
    </If>
  </View>;
}
mount(Fixture);
