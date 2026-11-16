import { mount } from "nsh";
// Allo (SIL OFL) is the theme's font-sans; each weight the UI uses ships as its own face.
import "./assets/fonts/allo/Allo-Regular.otf";
import "./assets/fonts/allo/Allo-Medium.otf";
import "./assets/fonts/allo/Allo-SemiBold.otf";
import "./assets/fonts/allo/Allo-Bold.otf";
import { AppShell } from "./src/shell/AppShell";
mount(AppShell);
