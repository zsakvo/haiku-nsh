import { signal, Text, View } from "nsh";
import { Button } from "../ui/Button";
import { Field } from "../ui/LibraryControls";
import {
  SettingCardBlock,
  SettingRow,
  SettingSection,
  SettingSelectRow,
  SettingSlider,
  SettingSwitchRow,
} from "../ui/SettingsControls";
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
  return <SettingSection testID="desktop-lyrics-settings" title="桌面歌词">
    <SettingSwitchRow label="显示桌面歌词" checked={d().visible} onChange={visible=>patch({visible})} />
    <SettingSwitchRow divider label="锁定桌面歌词" description="锁定后鼠标穿透，悬停时可解锁。" checked={d().locked} onChange={locked=>patch({locked})} />
    <SettingSelectRow divider label="桌面歌词布局" value={d().layout} items={[
      {value:"classic",title:"经典"},{value:"focus",title:"焦点"},{value:"left",title:"左对齐"},{value:"stage",title:"舞台"},
    ]} onChange={layout=>patch({layout:layout as Preferences["layout"]})} />
    <SettingRow divider title="桌面歌词字体" description={d().font || "系统字体"}>
      <Button label="选择桌面歌词字体…" size="sm" onPress={()=>void (async()=>{
        try {await props.settings.core()!.settings.pickDesktopFont();error.set("");}catch(e){error.set(String(e));}
      })()} />
    </SettingRow>
    <SettingCardBlock divider>
      <SettingSlider label="桌面歌词字号" testID="desktop-lyrics-font-size" value={d().fontSize} min={14} max={60} step={1} onChange={fontSize=>patch({fontSize})} />
    </SettingCardBlock>
    <SettingSelectRow divider label="桌面歌词字重" value={d().fontWeight} items={[
      {value:"light",title:"细体"},{value:"regular",title:"常规"},{value:"medium",title:"中等"},{value:"semibold",title:"半粗"},{value:"bold",title:"粗体"},
    ]} onChange={fontWeight=>patch({fontWeight:fontWeight as Preferences["fontWeight"],fontName:""})} />
    <SettingCardBlock divider>
      <View class="flex-row gap-3">
        <View class="flex-1"><Field label="桌面歌词文字颜色" value={text()??d().textColor} onChange={v=>text.set(v)} /></View>
        <View class="flex-1"><Field label="桌面歌词高亮颜色" value={highlight()??d().highlightColor} onChange={v=>highlight.set(v)} /></View>
      </View>
      <View class="flex-row"><Button label="保存桌面歌词颜色" size="sm" onPress={saveColors} /></View>
      <If when={!!error()}><Text class="text-sm text-destructive">{error()}</Text></If>
    </SettingCardBlock>
    <SettingSwitchRow divider label="桌面歌词描边" checked={d().outline} onChange={outline=>patch({outline})} />
    <SettingSwitchRow divider label="桌面歌词阴影" checked={d().shadow??d().outline} onChange={shadow=>patch({shadow})} />
    <SettingSwitchRow divider label="桌面歌词显示译文" checked={d().showTranslation} onChange={showTranslation=>patch({showTranslation})} />
    <SettingSelectRow divider label="桌面歌词背景" value={d().background} items={[
      {value:"none",title:"透明"},{value:"solid",title:"纯色"},{value:"liquid",title:"系统模糊"},
    ]} onChange={background=>patch({background:background as Preferences["background"]})} />
  </SettingSection>;
}
