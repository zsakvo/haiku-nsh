import { cleanup, keymap, mount } from "nsh";
import { Menu } from "nsh/desktop";
import { ActivityFixture } from "./src/pages/fixtures/ActivityFixture";

mount(() => {
  // Headless can select popup menus but cannot open the native application menu.
  // Expose the actual registered menu through a fixture-only shortcut.
  const setApplicationMenu = Menu.setApplicationMenu;
  let items: Parameters<typeof setApplicationMenu>[0] = null;
  Menu.setApplicationMenu = (next) => {
    items = next;
    setApplicationMenu(next);
  };
  cleanup(() => { Menu.setApplicationMenu = setApplicationMenu; });
  keymap.bind("ctrl+alt+shift+m", () => {
    if (items) void Menu.popup(items);
  }, { when: "always" });
  return <ActivityFixture controls />;
});
