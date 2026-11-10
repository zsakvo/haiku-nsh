import { mount } from "nsh";
import { fileURLToPath } from "url";
import { EffectsSettingsPreview } from "./src/pages/EffectsSettingsPreview";
mount(EffectsSettingsPreview, { category: "播放", artifacts: fileURLToPath(new URL("./.artifacts/m1-05", import.meta.url)) });
