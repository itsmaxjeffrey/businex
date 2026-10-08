import { expect, test, type Page } from "@playwright/test";

// Smoke E2E for the Businex desktop app: real registration, company
// creation, invitation issuance, sign-out and sign-in against a local API
// instance and the production bundle. In-page document-load numbers are
// printed as E2E_PERF lines for the implementation tracker.

const STAMP = Date.now();

interface Perf {
  dclMs: number;
  loadMs: number;
  fcpMs: number;
}

async function measure(page: Page): Promise<Perf> {
  return page.evaluate(() => {
    const nav = performance.getEntriesByType("navigation")[0] as PerformanceNavigationTiming;
    const fcp = performance.getEntriesByName("first-contentful-paint")[0];
    return {
      dclMs: Math.round(nav.domContentLoadedEventEnd),
      loadMs: Math.round(nav.loadEventEnd),
      fcpMs: Math.round(fcp ? fcp.startTime : 0)
    };
  });
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

  // Invite a teammate; the single-use token is surfaced exactly once.
  await page.getByRole("link", { name: "Team", exact: true }).click();
  await page.getByRole("button", { name: "Invite member", exact: true }).click();
  await page.getByLabel(/Email/).fill("invitee-" + STAMP + "@example.test");
  await page.getByRole("button", { name: "Send invite", exact: true }).click();
  await expect(page.getByText(/expires at/)).toBeVisible();
  await page.getByRole("button", { name: "Done", exact: true }).click();

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
  await page.reload();
  await expect(
    page.getByRole("link", { name: "Team", exact: true })
  ).toBeVisible();
  const warm = await measure(page);
  console.log("E2E_PERF warm-home " + JSON.stringify(warm));
  expect(warm.loadMs).toBeLessThan(5000);
});