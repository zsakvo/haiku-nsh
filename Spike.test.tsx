import { expect, open, test } from "nsh/test";

test("runs 1000 concurrent plugin echoes with NSH computation and VM isolation", async () => {
  const app = await open("./tests/fixtures/QuickJs.tsx", { platform: "macos" });
  await app.press("role:button[Run 1000 echoes]");
  await app.waitUntil(
    () => app.texts().some((text) => text.startsWith("Passed") || text.startsWith("Failed:")),
    { timeout: 120_000 },
  );
  expect(app.texts()).toContain("Passed 1000 echoes with overlapping JS computation");
}, 130_000);
