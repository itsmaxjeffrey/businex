import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { DESKTOP, MOBILE } from "./devices";

// Real two-user team journey against the local API and the production
// bundle: invitation issuance (single-use, email-bound token), acceptance
// by the invited account, role management with a privilege denial, and
// removal with immediate access loss. Runs once per project so the same
// flow is proven on the desktop and on the phone layout.

function unique(tag: string): string {
  return tag + "-" + Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
}

async function register(page: Page, email: string, name: string): Promise<void> {
  await page.goto("/#/register");
  await page.getByLabel(/Email/).fill(email);
  await page.getByLabel(/^Name/).fill(name);
  await page.getByLabel(/Password/).fill("correct horse battery");
  await page.getByRole("button", { name: "Create account", exact: true }).click();
  await expect(page.getByRole("link", { name: "Team", exact: true })).toBeVisible();
}

test("team journey: invite, accept, role change, denial, removal", async ({ page, browser }) => {
  // Two full registrations plus several dialog round trips; the emulated
  // phone runs this journey well inside a minute but not inside 30 s.
  test.setTimeout(60_000);
  const ownerEmail = unique("owner") + "@example.test";
  const memberEmail = unique("member") + "@example.test";

  // Owner registers and creates the company.
  await register(page, ownerEmail, "Owner User");
  await page.getByRole("button", { name: "Create company", exact: true }).click();
  await page.getByLabel(/Company name/).fill("Team Co " + unique("co"));
  await page.getByRole("button", { name: "Create", exact: true }).click();
  await expect(page.getByText("Company created.")).toBeVisible();

  // Invite the teammate; the single-use token is surfaced exactly once.
  await page.getByRole("link", { name: "Team", exact: true }).click();
  await page.getByRole("button", { name: "Invite member", exact: true }).click();
  const inviteDialog = page.getByRole("dialog", { name: "Invite a teammate" });
  await inviteDialog.getByLabel(/Email/).fill(memberEmail);
  await inviteDialog.getByLabel("Role", { exact: true }).selectOption("member");
  await inviteDialog.getByRole("button", { name: "Send invite", exact: true }).click();
  await expect(inviteDialog.getByText(/expires at/)).toBeVisible();
  const token = (await inviteDialog.locator("p.bx-code").innerText()).trim();
  expect(token.length).toBeGreaterThan(10);
  await inviteDialog.getByRole("button", { name: "Done", exact: true }).click();

  // The invited person registers with the invited address and joins.
  const profile = test.info().project.name === "mobile" ? MOBILE : DESKTOP;
  const teammateCtx: BrowserContext = await browser.newContext({
    ...profile,
    baseURL: test.info().project.use.baseURL
  });
  const teammate = await teammateCtx.newPage();
  await register(teammate, memberEmail, "Team Member");
  await teammate.getByRole("button", { name: "Accept invitation", exact: true }).click();
  const acceptDialog = teammate.getByRole("dialog", { name: "Accept invitation" });
  await acceptDialog.getByLabel(/Invitation token/).fill(token);
  await acceptDialog.getByRole("button", { name: "Join company", exact: true }).click();
  await expect(teammate.getByText(/Joined company .* as member/)).toBeVisible();
  // Success toasts clear themselves; wait for the clearance so the fixed
  // toast stack can never sit over the next control on the narrow layout.
  await expect(teammate.getByText(/Joined company/)).toBeHidden({ timeout: 10_000 });

  // The token is single-use: accepting it again is refused.
  await teammate.getByRole("button", { name: "Accept invitation", exact: true }).click();
  await acceptDialog.getByLabel(/Invitation token/).fill(token);
  await acceptDialog.getByRole("button", { name: "Join company", exact: true }).click();
  await expect(acceptDialog.getByRole("alert")).toContainText(/already used/);
  await teammate.keyboard.press("Escape");

  // The owner sees the new member and narrows their role to viewer.
  await page.reload();
  await expect(page.getByText(memberEmail)).toBeVisible();
  const memberRow = page.locator("tr", { hasText: memberEmail });
  await expect(memberRow.getByText("member", { exact: true })).toBeVisible();
  await memberRow.getByLabel("Role for Team Member").selectOption("viewer");
  await expect(page.getByText("Team Member is now viewer.")).toBeVisible();
  await expect(memberRow.getByText("viewer", { exact: true })).toBeVisible();
  await expect(page.getByText("Team Member is now viewer.")).toBeHidden({ timeout: 10_000 });

  // A viewer cannot manage members: self-elevation is refused by the API
  // and the change leaves the badge untouched.
  await teammate.goto("/#/team");
  const teammateRow = teammate.locator("tr", { hasText: memberEmail });
  await teammateRow.getByLabel("Role for Team Member").selectOption("admin");
  await expect(teammate.getByText(/role viewer does not grant members.manage/)).toBeVisible();
  await expect(teammateRow.getByText("viewer", { exact: true })).toBeVisible();

  // The same refusal protects the membership itself: the viewer cannot
  // remove their own row, and it stays put. The first error toast is
  // dismissed before the attempt so the second denial is a fresh signal.
  await teammate.getByRole("button", { name: "Dismiss", exact: true }).click();
  await teammateRow.getByRole("button", { name: "Remove", exact: true }).click();
  const teammateRemove = teammate.getByRole("dialog", { name: "Remove member" });
  await teammateRemove.getByRole("button", { name: "Remove", exact: true }).click();
  await expect(teammate.getByText(/role viewer does not grant members.manage/)).toBeVisible();
  await expect(teammateRow.getByText("viewer", { exact: true })).toBeVisible();
  await teammate.keyboard.press("Escape");

  // Removal drops the member immediately: the row disappears and the
  // removed account is left with no company to manage.
  await memberRow.getByRole("button", { name: "Remove", exact: true }).click();
  const removeDialog = page.getByRole("dialog", { name: "Remove member" });
  await expect(removeDialog.getByText(/^Remove Team Member from this company/)).toBeVisible();
  await removeDialog.getByRole("button", { name: "Remove", exact: true }).click();
  await expect(page.getByText("Team Member was removed.")).toBeVisible();
  await expect(page.getByText(memberEmail)).toHaveCount(0);

  await teammate.reload();
  await expect(teammate.getByText("Create or join a company first")).toBeVisible();
  await teammateCtx.close();
});
