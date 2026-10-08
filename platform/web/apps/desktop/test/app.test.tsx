import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { App } from "../src/App";
import { jsonResponse, stubFetch } from "./helpers";

const ME = {
  user: { id: "u1", email: "a@b.test", name: "Ada" },
  companies: [{ id: "c1", role: "owner" }]
};

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("App", () => {
  it("shows the sign-in screen when there is no session", async () => {
    stubFetch((url) => {
      if (url === "/api/auth/me") {
        return jsonResponse(401, { error: "unauthorized" });
      }
      return jsonResponse(404, { error: "not found" });
    });
    render(() => <App />);
    expect(await screen.findByRole("button", { name: "Sign in" })).toBeTruthy();
  });

  it("shows the shell with navigation when a session loads", async () => {
    stubFetch((url) => {
      if (url === "/api/auth/me") return jsonResponse(200, ME);
      return jsonResponse(404, { error: "not found" });
    });
    render(() => <App />);
    expect(await screen.findByRole("link", { name: "Team" })).toBeTruthy();
    expect(screen.getByText("Where the business stands right now.")).toBeTruthy();
  });

  it("signing out returns to the sign-in screen", async () => {
    stubFetch((url) => {
      if (url === "/api/auth/me") return jsonResponse(200, ME);
      if (url === "/api/auth/logout") return jsonResponse(200, {});
      return jsonResponse(404, { error: "not found" });
    });
    render(() => <App />);
    await screen.findByRole("link", { name: "Team" });
    fireEvent.click(screen.getByRole("button", { name: "Sign out" }));
    expect(await screen.findByRole("button", { name: "Sign in" })).toBeTruthy();
  });
});