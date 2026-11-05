import { defineConfig } from "@playwright/test";

// Documentation screenshots, not a test suite: npx playwright test -c scripts/docs-screenshots.config.ts
export default defineConfig({
  testDir: ".",
  testMatch: ["docs-screenshots.spec.ts"],
  timeout: 60_000,
  expect: { timeout: 15_000 },
  use: {
    baseURL: "http://127.0.0.1:1420",
    viewport: { width: 1440, height: 900 },
    deviceScaleFactor: 2,
    trace: "off",
    screenshot: "off",
    video: "off",
  },
  workers: 1,
  webServer: {
    command: "npm run dev",
    url: "http://127.0.0.1:1420",
    reuseExistingServer: true,
    cwd: "..",
  },
  reporter: [["list"]],
});
