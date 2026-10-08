import { defineConfig } from "@playwright/test";
import { DESKTOP, MOBILE } from "./e2e/devices";

// E2E runs against the production bundle (vite preview) and a disposable
// local API instance. Origins come from the environment so no deployed
// endpoint can ever be the default target.
const baseURL = process.env.BUSINEX_WEB_ORIGIN ?? "http://127.0.0.1:4174";

// Every spec runs once per project: a wide desktop and a Pixel-7-class
// phone. Both engines are Chrome so the two runs differ only in device
// emulation, and multi-user specs reuse the same profiles for their
// secondary contexts.
export default defineConfig({
  testDir: "./e2e",
  timeout: 30_000,
  workers: 1,
  reporter: [["list"]],
  use: {
    channel: "chrome",
    baseURL,
    trace: "retain-on-failure"
  },
  projects: [
    { name: "desktop", use: DESKTOP },
    { name: "mobile", use: MOBILE }
  ]
});
