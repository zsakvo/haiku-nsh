import { Text, token, View, type JSX } from "nsh";
import { Button as BaseButton, Select as BaseSelect, Slider as BaseSlider, Switch as BaseSwitch } from "nsh/base";
import { focus } from "./Button";
import { Icon } from "./Icon";
import { nativeMenus, nativePopup, popupItem, popupSurface } from "./Surfaces";

export function SettingSection(props: {
  title?: string;
  description?: string;
  action?: JSX.Element;
  children?: JSX.Element;
  testID?: string;
}) {
  return (
    <View testID={props.testID} class="flex-col gap-1.5">
      <If when={!!props.title || !!props.action}>
        <View class="flex-row items-center justify-between gap-3 px-1.5">
          <If when={!!props.title}>
            <Text role="header" class="font-medium text-muted-foreground" style={{ fontSize: token("sidebar-hint-size") }}>
              {props.title}
            </Text>
          </If>
          <If when={!props.title}><View /></If>
          {props.action}
        </View>
      </If>
      <View class="flex-col overflow-hidden rounded-(--radius-panel) bg-settings-card">
        {props.children}
      </View>
      <If when={!!props.description}>
        <Text class="px-1.5 text-muted-foreground" style={{ fontSize: token("sidebar-hint-size") }}>{props.description}</Text>
      </If>
    </View>
  );
}

export function SettingDivider() {
  return <View class="mx-3 h-px bg-settings-divider" />;
}

export function SettingRow(props: {
  title: string;
  description?: string;
  children?: JSX.Element;
  divider?: boolean;
  testID?: string;
}) {
  return (
    <>
      <If when={!!props.divider}><SettingDivider /></If>
      <View testID={props.testID} class="min-h-10 flex-row items-center justify-between gap-4 px-3 py-2">
        <View class="min-w-0 flex-1 flex-col gap-0.5">
          <Text class="text-foreground" style={{ fontSize: token("sidebar-text-size") }}>{props.title}</Text>
          <If when={!!props.description}>
            <Text class="text-muted-foreground" style={{ fontSize: token("sidebar-hint-size") }}>{props.description}</Text>
          </If>
        </View>
        <View class="shrink-0 flex-row items-center gap-2">{props.children}</View>
      </View>
    </>
  );
}

export function SettingCardBlock(props: {
  children?: JSX.Element;
  divider?: boolean;
  class?: string;
  testID?: string;
}) {
  return (
    <>
      <If when={!!props.divider}><SettingDivider /></If>
      <View testID={props.testID} class={`flex-col gap-2.5 px-3 py-2.5 ${props.class ?? ""}`}>
        {props.children}
      </View>
    </>
  );
}

export function SettingSwitch(props: {
  label: string;
  hideLabel?: boolean;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <BaseSwitch
      checked={props.checked}
      onChange={(e) => props.onChange(e.checked)}
      accessibilityLabel={props.label}
      class={`flex-row items-center gap-2 rounded-full ${focus}`}
      trackClass="h-4.5 w-8 justify-center rounded-full bg-settings-switch-track p-0.5 transition-colors duration-200 ease-out motion-reduce:transition-none checked:bg-primary disabled:opacity-40"
      thumbClass="size-3.5 rounded-full bg-settings-switch-thumb shadow-sm transition-transform duration-200 ease-out motion-reduce:transition-none checked:translate-x-3.5"
    >
      <If when={!props.hideLabel}>
        <Text class="text-foreground" style={{ fontSize: token("control-text-size") }}>{props.label}</Text>
      </If>
    </BaseSwitch>
  );
}

export function SettingSwitchRow(props: {
  label: string;
  description?: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  divider?: boolean;
}) {
  return (
    <SettingRow title={props.label} description={props.description} divider={props.divider}>
      <SettingSwitch label={props.label} hideLabel checked={props.checked} onChange={props.onChange} />
    </SettingRow>
  );
}

export function SettingSlider(props: {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  disabled?: boolean;
  onChange: (value: number) => void;
  testID?: string;
  format?: (value: number) => string;
}) {
  return (
    <View class="min-w-0 gap-1">
      <View class="flex-row justify-between gap-3">
        <Text class="text-xs text-muted-foreground">{props.label}</Text>
        <Text class="text-xs text-foreground">{props.format ? props.format(props.value) : props.value.toFixed(1)}</Text>
      </View>
      <BaseSlider
        testID={props.testID}
        accessibilityLabel={props.label}
        value={props.value}
        min={props.min}
        max={props.max}
        step={props.step ?? 0.5}
        disabled={props.disabled}
        onChange={(e) => {
          if (typeof e.value === "number") props.onChange(e.value);
        }}
        class="w-full disabled:opacity-40"
        trackClass="h-4 justify-center"
        indicatorClass="h-1 rounded-full bg-settings-switch-track"
        rangeClass="rounded-full bg-foreground"
        thumbClass={`size-3 rounded-full border border-border bg-card shadow-sm active:bg-muted ${focus}`}
      />
    </View>
  );
}

/**
 * A pop-up button: the system NSMenu on macOS, a drawn Select elsewhere.
 * `inline` is the borderless System Settings form whose value reads one step below the row title.
 */
export function SettingSelect(props: {
  label: string;
  value: string;
  items: { value: string; title: string }[];
  onChange: (value: string) => void;
  inline?: boolean;
  disabled?: boolean;
}) {
  const currentTitle = () => props.items.find((i) => i.value === props.value)?.title ?? "自定义";
  const triggerClass = props.inline
    ? `min-h-6 flex-row items-center gap-1 rounded-(--radius-inset) px-1.5 py-0.5 active:bg-muted selected:bg-muted disabled:opacity-40 ${focus}`
    : `min-h-6 min-w-32 flex-row items-center justify-between gap-2 rounded-(--radius-inset) border border-border bg-card px-2 py-0.5 shadow-sm active:bg-muted selected:bg-muted disabled:opacity-40 ${focus}`;
  const triggerBody = () => <>
    <Text class={props.inline ? "text-muted-foreground" : "text-foreground"} style={{
      numberOfLines: 1, fontSize: token(props.inline ? "setting-value-size" : "control-text-size"),
    }}>{currentTitle()}</Text>
    <Icon name="down" class="size-3" color="muted-foreground" />
  </>;

  let trigger: JSX.Element;
  if (nativeMenus) {
    const menu = nativePopup();
    trigger = (
      <View ref={menu.ref} class="shrink-0">
        <BaseButton accessibilityLabel={props.label} disabled={props.disabled} selected={menu.open()}
          onPress={() => void menu.show(props.items.map((item) => ({
            label: item.title,
            checked: item.value === props.value,
            onSelect: () => { if (item.value !== props.value) props.onChange(item.value); },
          })))}
          class={triggerClass}>
          {triggerBody()}
        </BaseButton>
      </View>
    );
  } else {
    trigger = (
      <BaseSelect
        accessibilityLabel={props.label}
        value={props.value}
        items={props.items}
        disabled={props.disabled}
        onChange={(e) => {
          if (e.value !== null) props.onChange(e.value);
        }}
        align={props.inline ? "end" : undefined}
        popupClass={`${popupSurface} min-w-40 p-1`}
        listClass="max-h-64"
        itemClass={popupItem}
        trigger={<View class={triggerClass}>{triggerBody()}</View>}
      />
    );
  }
  if (props.inline) return trigger;
  return (
    <View class="flex-row items-center justify-between gap-4">
      <Text class="text-foreground" style={{ fontSize: token("control-text-size") }}>{props.label}</Text>
      {trigger}
    </View>
  );
}

export function SettingSelectRow(props: {
  label: string;
  description?: string;
  value: string;
  items: { value: string; title: string }[];
  onChange: (value: string) => void;
  divider?: boolean;
}) {
  return (
    <SettingRow title={props.label} description={props.description} divider={props.divider}>
      <SettingSelect label={props.label} value={props.value} items={props.items} onChange={props.onChange} inline />
    </SettingRow>
  );
}
