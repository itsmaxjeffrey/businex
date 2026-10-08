import { afterEach, describe, expect, it, vi } from "vitest";
import { api, ApiError } from "../src/api";
import { jsonResponse } from "./helpers";

afterEach(() => vi.unstubAllGlobals());

describe("api client", () => {
  it("raises ApiError with the server-provided message", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => jsonResponse(403, { error: "forbidden thing" }))
    );
    const failure = api.me().catch((error: unknown) => error);
    expect(await failure).toBeInstanceOf(ApiError);
    expect(await failure).toMatchObject({ status: 403, message: "forbidden thing" });
  });

  it("falls back to a status message when the body carries no error", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => jsonResponse(500, {})));
    const failure = api.me().catch((error: unknown) => error);
    expect(await failure).toMatchObject({
      message: "Request failed with status 500"
    });
  });
});