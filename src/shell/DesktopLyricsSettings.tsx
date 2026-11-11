import { signal, Text, View } from "nsh";
import { Button } from "../ui/Button";
import { Field } from "../ui/LibraryControls";
import { SettingSelect, SettingSlider, SettingSwitch } from "../ui/SettingsControls";
import type { createSettings, DesktopLyricsSettings as Preferences } from "../state/settings";
/** Native surface preferences share the durable Rust settings authority. */
export function DesktopLyricsSettings(props:{settings:ReturnType<typeof createSettings>}) {
  const d=()=>props.settings.value()!.desktopLyrics;
  const patch=(desktopLyrics:Partial<Preferences>)=>void props.settings.update({desktopLyrics});
  const text=signal<string|null>(null),highlight=signal<string|null>(null),error=signal("");
  function saveColors(){
    const textColor=text()??d().textColor,highlightColor=highlight()??d().highlightColor;
    if([textColor,highlightColor].some(v=>v!=="system"&&!/^#[a-f0-9]{6}$/i.test(v))){error.set("颜色请使用 #RRGGBB，或 system 表示默认颜色");return;}
    error.set("");patch({textColor,highlightColor});
  }
  return <View testID="desktop-lyrics-settings" class="gap-3">
    <Text role="header" class="text-base font-semibold text-foreground">桌面歌词</Text>
    <SettingSwitch label="显示桌面歌词" checked={d().visible} onChange={visible=>patch({visible})} />
    <SettingSwitch label="锁定桌面歌词" checked={d().locked} onChange={locked=>patch({locked})} />
    <Text class="text-xs text-muted-foreground">锁定后鼠标穿透，悬停时可解锁。</Text>
    <SettingSelect label="桌面歌词布局" value={d().layout} items={[
      {value:"classic",title:"经典"},{value:"focus",title:"焦点"},{value:"left",title:"左对齐"},{value:"stage",title:"舞台"},
    ]} onChange={layout=>patch({layout:layout as Preferences["layout"]})} />
    <View class="flex-row items-center justify-between gap-3">
      <Text class="text-sm text-foreground">{d().font || "系统字体"}</Text>
      <Button label="选择桌面歌词字体…" onPress={()=>void (async()=>{
        try {await props.settings.core()!.settings.pickDesktopFont();error.set("");}catch(e){error.set(String(e));}
      })()} />
    </View>
    <SettingSlider label="桌面歌词字号" testID="desktop-lyrics-font-size" value={d().fontSize} min={14} max={60} step={1} onChange={fontSize=>patch({fontSize})} />
    <SettingSelect label="桌面歌词字重" value={d().fontWeight} items={[
      {value:"light",title:"细体"},{value:"regular",title:"常规"},{value:"medium",title:"中等"},{value:"semibold",title:"半粗"},{value:"bold",title:"粗体"},
    ]} onChange={fontWeight=>patch({fontWeight:fontWeight as Preferences["fontWeight"],fontName:""})} />
    <View class="flex-row gap-3"><View class="flex-1"><Field label="桌面歌词文字颜色" value={text()??d().textColor} onChange={v=>text.set(v)} /></View>
      <View class="flex-1"><Field label="桌面歌词高亮颜色" value={highlight()??d().highlightColor} onChange={v=>highlight.set(v)} /></View></View>
    <View class="flex-row"><Button label="保存桌面歌词颜色" onPress={saveColors} /></View>
    <SettingSwitch label="桌面歌词描边" checked={d().outline} onChange={outline=>patch({outline})} />
    <SettingSwitch label="桌面歌词阴影" checked={d().shadow??d().outline} onChange={shadow=>patch({shadow})} />
    <SettingSwitch label="桌面歌词显示译文" checked={d().showTranslation} onChange={showTranslation=>patch({showTranslation})} />
    <SettingSelect label="桌面歌词背景" value={d().background} items={[
      {value:"none",title:"透明"},{value:"solid",title:"纯色"},{value:"liquid",title:"系统模糊"},
    ]} onChange={background=>patch({background:background as Preferences["background"]})} />
    <If when={!!error()}><Text class="text-sm text-destructive">{error()}</Text></If>
  </View>;
}
