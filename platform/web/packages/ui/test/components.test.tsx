import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { Badge } from "../src/components/Badge";
import { Button } from "../src/components/Button";
import { EmptyState } from "../src/components/EmptyState";
import { Kpi } from "../src/components/Kpi";
import { SelectField } from "../src/components/SelectField";
import { TextField } from "../src/components/TextField";
import { ToastRegion } from "../src/components/ToastRegion";

afterEach(cleanup);

describe("Button", () => {
  it("renders its label as a real button defaulting to type=button", () => {
    const { getByRole } = render(() => <Button>Save changes</Button>);
    const button = getByRole("button", { name: "Save changes" });
    expect(button.getAttribute("type")).toBe("button");
  });

  it("applies the variant and size classes and reflects disabled state", () => {
    const { getByRole } = render(() => (
      <Button variant="danger" size="sm" disabled>
        Remove member
      </Button>
    ));
    const button = getByRole("button") as HTMLButtonElement;
    expect(button.className).toContain("bx-btn--danger");
    expect(button.className).toContain("bx-btn--sm");
    expect(button.disabled).toBe(true);
  });

  it("fires onClick when activated", async () => {
    const onClick = vi.fn();
    const { getByRole } = render(() => <Button onClick={onClick}>Invite</Button>);
    await fireEvent.click(getByRole("button"));
    expect(onClick).toHaveBeenCalledTimes(1);
  });
});

describe("TextField", () => {
  it("associates the label with the input", () => {
    const { getByLabelText } = render(() => <TextField label="Email" name="email" />);
    const input = getByLabelText("Email") as HTMLInputElement;
    expect(input.getAttribute("name")).toBe("email");
  });

  it("announces errors and marks the input invalid", () => {
    const { getByRole, getByLabelText } = render(() => (
      <TextField label="Password" name="password" error="Password must be at least 8 characters" />
    ));
    const error = getByRole("alert");
    expect(error.textContent).toContain("at least 8 characters");
    const input = getByLabelText("Password");
    expect(input.getAttribute("aria-invalid")).toBe("true");
    expect(input.getAttribute("aria-describedby")).toContain(error.id);
  });

  it("marks required fields in visible text", () => {
    const { getByText } = render(() => <TextField label="Email" required />);
    expect(getByText("(required)")).toBeTruthy();
  });
});

describe("SelectField", () => {
  it("renders options under a real label", () => {
    const { getByLabelText } = render(() => (
      <SelectField
        label="Role"
        name="role"
        options={[{ value: "admin", label: "Admin" }, { value: "member", label: "Member" }]}
      />
    ));
    const select = getByLabelText("Role") as HTMLSelectElement;
    expect(select.options.length).toBe(2);
    expect(select.options[0].textContent).toBe("Admin");
  });
});

describe("Badge and Kpi", () => {
  it("renders the badge label as text, not color alone", () => {
    const { getByText } = render(() => <Badge tone="success">Active</Badge>);
    expect(getByText("Active")).toBeTruthy();
  });

  it("separates the metric value from its unit", () => {
    const { getByText } = render(() => <Kpi label="Storage used" value="2.4" unit="GB" />);
    expect(getByText("2.4")).toBeTruthy();
    expect(getByText("GB")).toBeTruthy();
  });
});

describe("EmptyState", () => {
  it("explains what appears here and offers one next action", () => {
    const { getByText, getByRole } = render(() => (
      <EmptyState
        title="No teammates yet"
        body="People you invite will show up here."
        action={<Button>Invite a teammate</Button>}
      />
    ));
    expect(getByText("No teammates yet")).toBeTruthy();
    expect(getByText("People you invite will show up here.")).toBeTruthy();
    expect(getByRole("button", { name: "Invite a teammate" })).toBeTruthy();
  });
});

describe("ToastRegion", () => {
  it("announces politely and dismisses on request", async () => {
    const onDismiss = vi.fn();
    const { getByRole, getByText } = render(() => (
      <ToastRegion
        items={[{ id: "t1", tone: "success", text: "Invitation created" }]}
        onDismiss={onDismiss}
      />
    ));
    const region = getByRole("status");
    expect(region.getAttribute("aria-live")).toBe("polite");
    await fireEvent.click(getByText("Dismiss"));
    expect(onDismiss).toHaveBeenCalledWith("t1");
  });
});
