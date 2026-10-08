import { vi } from "vitest";

export interface RecordedCall {
  url: string;
  method: string;
  body: unknown;
}

export function jsonResponse(status: number, body: unknown): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: async () => JSON.stringify(body)
  } as unknown as Response;
}

/** Stub fetch with a router function; returns the recorded call list. */
export function stubFetch(
  route: (url: string, method: string, body: unknown) => Response
): RecordedCall[] {
  const calls: RecordedCall[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const method = init?.method ?? "GET";
      const body =
        typeof init?.body === "string" ? JSON.parse(init.body) : undefined;
      calls.push({ url, method, body });
      return route(url, method, body);
    })
  );
  return calls;
}