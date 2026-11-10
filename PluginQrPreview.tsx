import { mount } from "nsh";
import { PluginSettingsPreview } from "./src/pages/PluginSettingsPreview";
mount(PluginSettingsPreview, { category: "来源", login: true, loginPlugin: "auth.fixture" });
