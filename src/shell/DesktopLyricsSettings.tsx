import { signal, Text, token } from "nsh";
import { Button } from "../ui/Button";
import { ColorPairPresets, ColorPickerButton } from "../ui/ColorControls";
import {
  SettingCardBlock,
  SettingRow,
  SettingSection,
  SettingSelectRow,
  SettingSlider,
  SettingSwitchRow,
} from "../ui/SettingsControls";
import type { createSettings, DesktopLyricsSettings as Preferences } from "../state/settings";
/** Text and highlight pairings that read on both light and dark desktops. */
const presets=[
  {name:"经典",first:"#FFFFFF",second:"#AC5E4F"},
  {name:"晴空",first:"#FFFFFF",second:"#4A9EFF"},
  {name:"薄荷",first:"#F2FFF8",second:"#34C38F"},
  {name:"落日",first:"#FFF6E8",second:"#FF8A3D"},
  {name:"樱粉",first:"#FFFFFF",second:"#FF6FA3"},
  {name:"鎏金",first:"#FFFFFF",second:"#E6B422"},
  {name:"紫雾",first:"#F5F0FF",second:"#9B7BFF"},
  {name:"墨夜",first:"#2B2B2B",second:"#E05A47"},
];
const label=(color:string)=>color==="system"?"跟随系统":color.toUpperCase();
/** The weight comes from the face picked in the system font panel; shown here, not edited. */
const weights:Record<Preferences["fontWeight"],string>={light:"细体",regular:"常规",medium:"中等",semibold:"半粗",bold:"粗体"};
/** Native surface preferences share the durable Rust settings authority. */
export function DesktopLyricsSettings(props:{settings:ReturnType<typeof createSettings>}) {
  const d=()=>props.settings.value()!.desktopLyrics;
  const patch=(desktopLyrics:Partial<Preferences>)=>void props.settings.update({desktopLyrics});
  const error=signal("");
  return <SettingSection testID="desktop-lyrics-settings" title="桌面歌词">
    <SettingSwitchRow label="显示桌面歌词" checked={d().visible} onChange={visible=>patch({visible})} />
    <SettingSwitchRow divider label="锁定桌面歌词" description="锁定后鼠标穿透，悬停时可解锁。" checked={d().locked} onChange={locked=>patch({locked})} />
    <SettingSelectRow divider label="桌面歌词布局" value={d().layout} items={[
      {value:"classic",title:"经典"},{value:"focus",title:"焦点"},{value:"left",title:"左对齐"},{value:"stage",title:"舞台"},
    ]} onChange={layout=>patch({layout:layout as Preferences["layout"]})} />
    <SettingRow divider title="桌面歌词字体" description={`${d().font || "系统字体"} · ${weights[d().fontWeight] ?? "常规"}`}>
      <Button label="选择桌面歌词字体…" size="sm" onPress={()=>void (async()=>{
        try {await props.settings.core()!.settings.pickDesktopFont();error.set("");}catch(e){error.set(String(e));}
      })()} />
    </SettingRow>
    <If when={!!error()}><Text class="px-3 pb-2 text-sm text-destructive">{error()}</Text></If>
    <SettingCardBlock divider>
      <SettingSlider label="桌面歌词字号" testID="desktop-lyrics-font-size" value={d().fontSize} min={14} max={60} step={1} onChange={fontSize=>patch({fontSize})} />
    </SettingCardBlock>
    <SettingCardBlock divider>
      <Text class="text-foreground" style={{ fontSize: token("sidebar-text-size") }}>桌面歌词配色</Text>
      <ColorPairPresets label="桌面歌词配色" presets={presets} first={d().textColor} second={d().highlightColor}
        onSelect={(textColor,highlightColor)=>patch({textColor,highlightColor})} />
    </SettingCardBlock>
    <SettingRow divider title="桌面歌词文字颜色" description={label(d().textColor)}>
      <ColorPickerButton label="桌面歌词文字颜色" value={d().textColor} fallback="#FFFFFF" onChange={textColor=>patch({textColor})} />
    </SettingRow>
    <SettingRow divider title="桌面歌词高亮颜色" description={label(d().highlightColor)}>
      <ColorPickerButton label="桌面歌词高亮颜色" value={d().highlightColor} fallback="#AC5E4F" onChange={highlightColor=>patch({highlightColor})} />
    </SettingRow>
    <SettingSwitchRow divider label="桌面歌词描边" checked={d().outline} onChange={outline=>patch({outline})} />
    <SettingSwitchRow divider label="桌面歌词阴影" checked={d().shadow??d().outline} onChange={shadow=>patch({shadow})} />
    <SettingSwitchRow divider label="桌面歌词显示译文" checked={d().showTranslation} onChange={showTranslation=>patch({showTranslation})} />
    <SettingSelectRow divider label="桌面歌词背景" value={d().background} items={[
      {value:"none",title:"透明"},{value:"solid",title:"纯色"},{value:"liquid",title:"系统模糊"},
    ]} onChange={background=>patch({background:background as Preferences["background"]})} />
  </SettingSection>;
}
