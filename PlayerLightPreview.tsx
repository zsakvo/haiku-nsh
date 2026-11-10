import { mount } from "nsh";
import { fileURLToPath } from "url";
import { PlayerPreview } from "./src/pages/PlayerPreview";
mount(PlayerPreview, { mode: "light", artifacts: fileURLToPath(new URL("./.artifacts/m1-04", import.meta.url)) });
