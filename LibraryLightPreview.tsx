import { mount } from "nsh";
import { fileURLToPath } from "url";
import { LibraryPreview } from "./src/pages/LibraryPreview";
mount(LibraryPreview, { mode: "light", artifacts: fileURLToPath(new URL("./.artifacts/m1-03", import.meta.url)) });
