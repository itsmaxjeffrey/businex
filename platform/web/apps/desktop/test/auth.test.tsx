import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { LoginPage } from "../src/pages/LoginPage";
import { jsonResponse, stubFetch } from "./helpers";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("LoginPage", () => {
  it("posts credentials and reloads the session", async () => {
    const calls = stubFetch((url) => {
      if (url === "/api/auth/login") {
        return jsonResponse(200, {
          user: { id: "u1", email: "a@b.test", name: "Ada" }
        });
      }
      if (url === "/api/auth/me") {
        return jsonResponse(200, {
          user: { id: "u1", email: "a@b.test", name: "Ada" },
          companies: [{ id: "c1", role: "owner" }]
        });
      }
      return jsonResponse(404, { error: "not found" });
    });

    const { getByLabelText, getByRole } = render(() => <LoginPage />);
    const email = getByLabelText(/Email/) as HTMLInputElement;
    const password = getByLabelText(/Password/) as HTMLInputElement;
    email.value = "a@b.test";
    fireEvent.input(email);
    password.value = "correct horse battery";
    fireEvent.input(password);
    const form = getByRole("button", { name: "Sign in" }).closest("form");
    expect(form).not.toBeNull();
    fireEvent.submit(form as HTMLFormElement);

    await vi.waitFor(() => {
      expect(calls.some((call) => call.url === "/api/auth/me")).toBe(true);
    });
    const login = calls.find((call) => call.url === "/api/auth/login");
    expect(login?.method).toBe("POST");
    expect(login?.body).toEqual({
      email: "a@b.test",
      password: "correct horse battery"
    });
  });

  it("shows the server error message on failure", async () => {
    stubFetch((url) => {
      if (url === "/api/auth/login") {
        return jsonResponse(401, { error: "invalid credentials" });
      }
      return jsonResponse(404, { error: "not found" });
    });

    const { getByLabelText, getByRole, findByRole } = render(() => (
      <LoginPage />
    ));
    (getByLabelText(/Email/) as HTMLInputElement).value = "a@b.test";
    fireEvent.input(getByLabelText(/Email/));
    (getByLabelText(/Password/) as HTMLInputElement).value = "wrong";
    fireEvent.input(getByLabelText(/Password/));
    const form = getByRole("button", { name: "Sign in" }).closest("form");
    fireEvent.submit(form as HTMLFormElement);

    const alert = await findByRole("alert");
    expect(alert.textContent).toContain("invalid credentials");
  });
});