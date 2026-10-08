import { defineConfig } from "@playwright/test";

// E2E runs against the production bundle (vite preview) and a disposable
// local API instance. Origins come from the environment so no deployed
// endpoint can ever be the default target.
const baseURL = process.env.BUSINEX_WEB_ORIGIN ?? "http://127.0.0.1:4174";

export default defineConfig({
  testDir: "./e2e",
  timeout: 30_000,
  workers: 1,
  reporter: [["list"]],
  use: {
    channel: "chrome",
    baseURL,
    trace: "retain-on-failure"
  }
});