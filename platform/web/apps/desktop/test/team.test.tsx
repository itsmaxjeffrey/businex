import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { TeamPage } from "../src/pages/TeamPage";
import { setActiveCompanyId } from "../src/state";
import { jsonResponse, stubFetch } from "./helpers";

const MEMBERS = {
  members: [
    { user_id: "u1", email: "owner@x.test", name: "Owner", role: "owner" },
    { user_id: "u2", email: "dev@x.test", name: "Dev", role: "member" }
  ]
};

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("TeamPage", () => {
  it("lists members of the active company", async () => {
    setActiveCompanyId("c1");
    stubFetch((url) => {
      if (url === "/api/companies/c1/members") {
        return jsonResponse(200, MEMBERS);
      }
      return jsonResponse(404, { error: "not found" });
    });

    render(() => <TeamPage />);
    await screen.findByText("Owner");
    expect(screen.getByText("dev@x.test")).toBeTruthy();
    // Owner rows carry the ownership-transfer rule instead of actions.
    expect(screen.getByText("Transferred by owner only")).toBeTruthy();
  });

  it("changing a role patches the member and refreshes the list", async () => {
    setActiveCompanyId("c1");
    const calls = stubFetch((url, method) => {
      if (url === "/api/companies/c1/members" && method === "GET") {
        return jsonResponse(200, MEMBERS);
      }
      if (url === "/api/companies/c1/members/u2" && method === "PATCH") {
        return jsonResponse(200, { userId: "u2", role: "viewer" });
      }
      return jsonResponse(404, { error: "not found" });
    });

    render(() => <TeamPage />);
    await screen.findByText("Dev");
    const select = screen.getByLabelText("Role for Dev") as HTMLSelectElement;
    select.value = "viewer";
    fireEvent.change(select);

    await vi.waitFor(() => {
      const patch = calls.find(
        (call) => call.method === "PATCH" && call.url.endsWith("/members/u2")
      );
      expect(patch).toBeDefined();
      expect(patch?.body).toEqual({ role: "viewer" });
    });
  });

  it("explains the empty case when no company is active", () => {
    setActiveCompanyId(null);
    render(() => <TeamPage />);
    expect(
      screen.getByText(/Create or join a company first/)
    ).toBeTruthy();
  });
});