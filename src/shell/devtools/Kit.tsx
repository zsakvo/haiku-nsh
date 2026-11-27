import { colorScheme, Image, Scroll, Text, token, View, type JSX } from "nsh";
import { Input } from "nsh/base";
import { PluginService } from "haiku/native";
import { Icon } from "../../ui/Icon";

/** The developer tools' own building blocks: panes with a header bar, a filled filter field that
 * cannot be mistaken for a button, tone pills, illustrated placeholders and monospaced text. */

export function Pane(props: {
  title?: string; accessory?: JSX.Element; children?: JSX.Element; class?: string; testID?: string;
}) {
  return <View testID={props.testID}
    class={`min-h-0 min-w-0 flex-col overflow-hidden rounded-(--radius-panel) border border-border bg-card ${props.class ?? ""}`}>
    <If when={!!props.title || !!props.accessory}>
      <View class="shrink-0 min-h-9 flex-row items-center gap-2 border-b border-border px-3 py-1">
        <Text role="header" class="flex-1 min-w-0 font-medium text-muted-foreground"
          style={{ fontSize: token("sidebar-hint-size"), numberOfLines: 1 }}>{props.title ?? ""}</Text>
        {props.accessory}
      </View>
    </If>
    {props.children}
  </View>;
}

/** A filled, borderless search field: inputs read as wells, buttons as raised surfaces. */
export function FilterField(props: { label: string; value: string; placeholder: string; onChange: (value: string) => void; class?: string }) {
  return <View class={`min-h-7 flex-row items-center gap-1.5 rounded-(--radius) border border-transparent bg-muted px-2 focus-within:border-ring ${props.class ?? ""}`}>
    <Icon name="search" class="size-3.5" color="muted-foreground" />
    <Input accessibilityLabel={props.label} value={props.value} placeholder={props.placeholder}
      onChange={(event) => props.onChange(event.value)} style={{ fontSize: token("control-text-size") }}
      class="flex-1 min-w-0 text-foreground placeholder:text-muted-foreground" />
  </View>;
}

export type Tone = "success" | "warning" | "destructive" | "primary" | "muted";
const pillTones: Record<Tone, string> = {
  success: "bg-success/15", warning: "bg-warning/15", destructive: "bg-destructive/15", primary: "bg-primary/15", muted: "bg-muted",
};
const textTones: Record<Tone, string> = {
  success: "text-success", warning: "text-warning", destructive: "text-destructive", primary: "text-primary", muted: "text-muted-foreground",
};
/** A short status word on a tinted capsule; the word carries the meaning, the tint only helps scanning. */
export function Pill(props: { tone: Tone; label: string; mono?: boolean }) {
  return <View class={`shrink-0 flex-row items-center rounded-full px-1.5 py-px ${pillTones[props.tone]}`}>
    <Text class={`${textTones[props.tone]} font-medium ${props.mono ? "font-mono" : ""}`}
      style={{ fontSize: token("sidebar-hint-size"), fontVariant: ["tabular-nums"] }}>{props.label}</Text>
  </View>;
}
export function Dot(props: { tone: Tone }) {
  const fill: Record<Tone, string> = { success: "bg-success", warning: "bg-warning", destructive: "bg-destructive", primary: "bg-primary", muted: "bg-muted-foreground" };
  return <View class={`size-1.5 shrink-0 rounded-full ${fill[props.tone]}`} />;
}

const art = {
  authoring: ["asset:/ui/illustrations/devtools-authoring.svg", "asset:/ui/illustrations/devtools-authoring-dark.svg"],
  debug: ["asset:/ui/illustrations/devtools-debug.svg", "asset:/ui/illustrations/devtools-debug-dark.svg"],
  network: ["asset:/ui/illustrations/devtools-network.svg", "asset:/ui/illustrations/devtools-network-dark.svg"],
  logs: ["asset:/ui/illustrations/devtools-logs.svg", "asset:/ui/illustrations/devtools-logs-dark.svg"],
} as const;
/** An illustrated empty state that explains what will appear and how to make it appear. */
export function Placeholder(props: {
  art: keyof typeof art; title: string; description: string; children?: JSX.Element; compact?: boolean;
}) {
  return <View class="flex-1 min-h-0 items-center justify-center gap-2 px-8 py-6">
    <Image src={art[props.art][colorScheme() === "dark" ? 1 : 0]}
      class={props.compact ? "h-28 w-40 mb-1" : "h-40 w-60 mb-2"} style={{ objectFit: "contain" }} />
    <Text role="header" class="text-base font-semibold text-foreground">{props.title}</Text>
    <Text class="max-w-96 text-center text-xs text-muted-foreground">{props.description}</Text>
    <If when={!!props.children}><View class="mt-2 flex-row gap-2">{props.children}</View></If>
  </View>;
}

/** Selectable monospaced text that scrolls on its own. */
export function CodeView(props: { text: string; class?: string; testID?: string }) {
  return <Scroll testID={props.testID} class={`flex-1 min-h-0 ${props.class ?? ""}`} contentClass="p-3">
    <View selectable><Text class="font-mono text-xs text-foreground">{props.text}</Text></View>
  </Scroll>;
}

/** Key–value rows in two aligned monospaced columns (headers, metadata). */
export function KeyValues(props: { entries: [string, string][]; empty?: string }) {
  return <View selectable class="gap-1">
    <If when={!props.entries.length}><Text class="text-xs text-muted-foreground">{props.empty ?? "无"}</Text></If>
    {props.entries.map(([key, value]) => <View class="flex-row gap-3">
      <Text class="w-44 shrink-0 font-mono text-xs text-muted-foreground" style={{ numberOfLines: 1, textOverflow: "ellipsis" }}>{key}</Text>
      <Text class="flex-1 min-w-0 font-mono text-xs text-foreground">{value}</Text>
    </View>)}
  </View>;
}

/** The system clipboard from any window. `Clipboard.writeText` addresses the root surface,
 * which a secondary window (the developer tools) does not reach, so copy natively. */
export function copyText(text: string) { return PluginService.copyText(text); }

/** JSON bodies pretty-printed; anything else as it came. */
export function prettyBody(text: string | undefined) {
  if (!text) return "";
  try { return JSON.stringify(JSON.parse(text), null, 2); } catch { return text; }
}
export function formatBytes(size: number | undefined) {
  if (size === undefined) return "";
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / 1024 / 1024).toFixed(1)} MB`;
}
export function clock(time: number | string, millis = false) {
  const d = new Date(time), pad = (v: number, n = 2) => String(v).padStart(n, "0");
  if (!Number.isFinite(d.getTime())) return "--:--:--";
  const base = `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
  return millis ? `${base}.${pad(d.getMilliseconds(), 3)}` : base;
}
