import { resource, Text, View } from "nsh";
import { Core } from "haiku/native";
import { AppShell } from "../../shell/AppShell";
/** Light isolated evidence; conflict text comes from the real validation path. */
export function ShortcutsFixture(props: { about?: boolean; conflict?: boolean }) {
  const ready=resource(async()=>{
    const core=await Core.openIsolated();
    let conflict="";
    if(props.conflict) {
      const snapshot=core.shortcuts.snapshot() as {bindings:{id:string;scope:string;chord:string}[]};
      try {await core.shortcuts.apply(snapshot.bindings.map(b=>b.scope==="global"&&b.id==="togglePlayback"?{...b,chord:"cmd+k"}:b));}
      catch(e){conflict=String(e);}
    }
    return {conflict};
  });
  return <View class="flex-1"><If when={!!ready()}><AppShell initialMode="light" initialOverlay="settings" initialSettingsCategory={props.about?"其他":"快捷键"} initialShortcutConflict={ready()!.conflict} /></If><If when={!!ready.error()}><Text>{String(ready.error())}</Text></If></View>;
}
