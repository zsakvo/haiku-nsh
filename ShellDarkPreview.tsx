import { mount } from "nsh";
import { AppShell } from "./src/shell/AppShell";
mount(AppShell, { initialMode: "dark" as const });
