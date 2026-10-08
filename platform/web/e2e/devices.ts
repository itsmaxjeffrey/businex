import { devices } from "@playwright/test";

// Shared browser-context profiles for the e2e suites. Playwright projects
// pick one for the main context, and multi-user tests reuse the same profile
// for secondary contexts so a mobile run stays mobile end to end — both
// sides of a team flow see the same layout.

const pixel = devices["Pixel 7"];

export const DESKTOP = {
  viewport: { width: 1440, height: 900 }
};

export const MOBILE = {
  userAgent: pixel.userAgent,
  viewport: pixel.viewport,
  deviceScaleFactor: pixel.deviceScaleFactor,
  isMobile: true,
  hasTouch: true
};
