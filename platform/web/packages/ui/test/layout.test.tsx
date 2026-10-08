import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@solidjs/testing-library";
import { AppShell } from "../src/components/AppShell";
import { DataTable, type Row } from "../src/components/DataTable";
import { Dialog } from "../src/components/Dialog";

afterEach(cleanup);

describe("DataTable", () => {
  const columns = [
    { key: "name", header: "Name" },
    { key: "email", header: "Email" },
    { key: "seats", header: "Seats", numeric: true }
  ];

  it("uses scoped headers and right-aligns numeric columns", () => {
    const rows: Row[] = [
      { id: "1", cells: { name: "Dana", email: "dana@example.test", seats: 3 } }
    ];
    const { getByRole, getByText } = render(() => (
      <DataTable caption="Company members" columns={columns} rows={rows} />
    ));
    expect(getByText("Company members")).toBeTruthy();
    const header = getByRole("columnheader", { name: "Seats" });
    expect(header.getAttribute("scope")).toBe("col");
    expect(header.className).toContain("bx-num");
    expect(getByText("3").className).toContain("bx-num");
  });

  it("falls back to an empty state when there are no rows", () => {
    const { getByText } = render(() => (
      <DataTable
        caption="Company members"
        columns={columns}
        rows={[]}
        emptyTitle="No members yet"
        emptyBody="Invite people to collaborate."
      />
    ));
    expect(getByText("No members yet")).toBeTruthy();
    expect(getByText("Invite people to collaborate.")).toBeTruthy();
  });
});

describe("Dialog", () => {
  it("labels the dialog with its title", () => {
    // Queried as an element rather than by role: a closed dialog is hidden
    // from the accessibility tree, which is exactly how it ships.
    const { container } = render(() => (
      <Dialog open={false} title="Remove this member?" onClose={() => {}} />
    ));
    const dialog = container.querySelector("dialog");
    expect(dialog).toBeTruthy();
    const labelledBy = dialog?.getAttribute("aria-labelledby");
    expect(labelledBy).toBeTruthy();
    const title = dialog?.querySelector("#" + labelledBy);
    expect(title?.textContent).toBe("Remove this member?");
  });
});

describe("AppShell", () => {
  it("marks the current nav item and exposes main and nav landmarks", () => {
    const { getByRole } = render(() => (
      <AppShell
        brand="Businex"
        nav={[{ label: "Home", href: "#/" }, { label: "Team", href: "#/team" }]}
        current="#/team"
      >
        <p>Team content</p>
      </AppShell>
    ));
    const nav = getByRole("navigation", { name: "Main" });
    const current = nav.querySelector("[aria-current=page]");
    expect(current?.textContent).toBe("Team");
    expect(getByRole("main")).toBeTruthy();
  });
});
