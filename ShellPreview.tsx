// Reproducible screenshot entry; keep next to nsh.json for the mixed host.
import { mount } from "nsh";
import { AppShell } from "./src/shell/AppShell";
mount(AppShell, { initialMode: "light" as const });
