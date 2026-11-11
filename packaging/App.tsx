import { mount } from "nsh";
import { AppShell } from "../src/shell/AppShell";

// Release entry: the adjacent manifest grants no fixture filesystem access.
mount(AppShell);
