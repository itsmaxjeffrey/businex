import { expect, test, type Page } from "@playwright/test";

// Smoke E2E for the Businex desktop app: real registration, company
// creation, invitation issuance, sign-out and sign-in against a local API
// instance and the production bundle. Document-load numbers are printed as
// E2E_PERF lines — local-lab document load only, never presented as
// real-user metrics such as LCP or INP.

// Unique per test run: the same file executes once per Playwright project
// (desktop and mobile) against one shared API, so accounts must not collide.
const STAMP = Date.now() + "-" + Math.random().toString(36).slice(2, 8);

interface Perf {
  dclMs: number;
  loadMs: number;
  fcpMs: number | "unavailable";
}

// Measure only after the navigation has actually completed: an unfinished
// load must never read as a fast one. First contentful paint is awaited
// briefly and reported as unavailable when it has not fired, never as 0.
async function measure(page: Page): Promise<Perf> {
  await page.waitForFunction(() => {
    const entries = performance.getEntriesByType("navigation");
    const nav = entries[0] as PerformanceNavigationTiming | undefined;
    return nav !== undefined && nav.loadEventEnd > 0;
  });
  await page
    .waitForFunction(
      () => performance.getEntriesByName("first-contentful-paint").length > 0,
      undefined,
      { timeout: 3000 }
    )
    .catch(() => undefined);
  const raw = await page.evaluate(() => {
    const nav = performance.getEntriesByType("navigation")[0] as PerformanceNavigationTiming;
    const fcp = performance.getEntriesByName("first-contentful-paint")[0];
    return {
      dclMs: Math.round(nav.domContentLoadedEventEnd),
      loadMs: Math.round(nav.loadEventEnd),
      fcpMs: fcp ? Math.round(fcp.startTime) : null
    };
  });
  return {
    dclMs: raw.dclMs,
    loadMs: raw.loadMs,
    fcpMs: raw.fcpMs === null ? "unavailable" : raw.fcpMs
  };
}

test("user journey: register, company, invite, sign out, sign in", async ({ page }) => {
  const email = "e2e-" + STAMP + "@example.test";
  const password = "correct horse battery";

  // Cold document load on the registration screen.
  await page.goto("/#/register");
  await expect(
    page.getByRole("button", { name: "Create account", exact: true })
  ).toBeVisible();
  const cold = await measure(page);
  console.log("E2E_PERF cold-register " + JSON.stringify(cold));
  expect(cold.dclMs).toBeGreaterThan(0);
  expect(cold.loadMs).toBeGreaterThan(0);
  expect(cold.loadMs).toBeLessThan(5000);

  await page.getByLabel(/Email/).fill(email);
  await page.getByLabel(/^Name/).fill("E2E User");
  await page.getByLabel(/Password/).fill(password);
  await page
    .getByRole("button", { name: "Create account", exact: true })
    .click();

  // Signed-in shell appears after registration.
  await expect(
    page.getByRole("link", { name: "Team", exact: true })
  ).toBeVisible();

  // Create a company from the home screen.
  await page.getByRole("button", { name: "Create company", exact: true }).click();
  await page.getByLabel(/Company name/).fill("E2E Co " + STAMP);
  await page.getByRole("button", { name: "Create", exact: true }).click();
  await expect(page.getByText("Company created.")).toBeVisible();

  // Invite a teammate. The single-use token must be surfaced exactly once:
  // after the dialog is dismissed it must not come back when the form is
  // reopened or after a reload.
  await page.getByRole("link", { name: "Team", exact: true }).click();
  await page.getByRole("button", { name: "Invite member", exact: true }).click();
  await page.getByLabel(/Email/).fill("invitee-" + STAMP + "@example.test");
  await page.getByRole("button", { name: "Send invite", exact: true }).click();
  await expect(page.getByText(/expires at/)).toBeVisible();
  const token = (await page
    .getByRole("dialog", { name: "Invite a teammate" })
    .locator("p.bx-code")
    .innerText()
  ).trim();
  expect(token.length).toBeGreaterThan(10);
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await expect(
    page.getByRole("dialog", { name: "Invite a teammate" })
  ).toBeHidden();

  // Reopening the form starts a fresh invite: the old token is gone.
  await page.getByRole("button", { name: "Invite member", exact: true }).click();
  await expect(page.getByText(/expires at/)).toBeHidden();
  await expect(page.getByText(token)).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(
    page.getByRole("dialog", { name: "Invite a teammate" })
  ).toBeHidden();

  // A reload does not bring the token back either.
  await page.reload();
  await expect(
    page.getByRole("link", { name: "Team", exact: true })
  ).toBeVisible();
  await expect(page.getByText(token)).toHaveCount(0);
  await page.getByRole("button", { name: "Invite member", exact: true }).click();
  await expect(page.getByText(token)).toHaveCount(0);
  await page.keyboard.press("Escape");

  // Keyboard activation opens the dialog and Escape dismisses it again.
  await page.getByRole("button", { name: "Invite member", exact: true }).focus();
  await page.keyboard.press("Enter");
  await expect(
    page.getByRole("dialog", { name: "Invite a teammate" })
  ).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(
    page.getByRole("dialog", { name: "Invite a teammate" })
  ).toBeHidden();

  // Sign out returns to the sign-in screen.
  await page.getByRole("button", { name: "Sign out", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Sign in", exact: true })
  ).toBeVisible();

  // The same credentials work again.
  await page.getByLabel(/Email/).fill(email);
  await page.getByLabel(/Password/).fill(password);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(
    page.getByRole("link", { name: "Team", exact: true })
  ).toBeVisible();

  // Warm document load: a real second page load with the cache populated.
  // The signed-in shell surviving the reload proves session persistence.
  await page.reload();
  await expect(
    page.getByRole("link", { name: "Team", exact: true })
  ).toBeVisible();
  const warm = await measure(page);
  console.log("E2E_PERF warm-home " + JSON.stringify(warm));
  expect(warm.dclMs).toBeGreaterThan(0);
  expect(warm.loadMs).toBeGreaterThan(0);
  expect(warm.loadMs).toBeLessThan(5000);
});
