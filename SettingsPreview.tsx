import { mount } from "nsh";
import { AppShell } from "./src/shell/AppShell";
mount(AppShell, { initialMode: "light" as const, initialOverlay: "settings" as const });
