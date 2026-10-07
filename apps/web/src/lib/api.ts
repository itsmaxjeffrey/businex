// Small typed fetch client: session cookie auth + workspace scoping.

import { isDemoMode, demoRequest } from "./demo";

let workspaceId: string | null = null;

export function setWorkspaceId(id: string | null): void {
  workspaceId = id;
}

export function getWorkspaceId(): string | null {
  return workspaceId;
}

export class ApiError extends Error {
  status: number;
  code: string;
  constructor(status: number, code: string, message: string) {
    super(message);
    this.status = status;
    this.code = code;
  }
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  // Demo mode serves the real UI from an in-memory workspace (no backend).
  if (isDemoMode()) {
    const demo = await demoRequest(method, path, body);
    if (demo.status >= 400) {
      throw new ApiError(demo.status, demo.data?.error?.code ?? "error", demo.data?.error?.message ?? "Request failed");
    }
    return demo.data as T;
  }

  const headers: Record<string, string> = {};
  if (body !== undefined) headers["Content-Type"] = "application/json";
  if (workspaceId) headers["X-Workspace-Id"] = workspaceId;

  const res = await fetch("/api" + path, {
    method,
    headers,
    credentials: "same-origin",
    body: body === undefined ? undefined : JSON.stringify(body),
  });

  const text = await res.text();
  const data = text ? JSON.parse(text) : null;
  if (!res.ok) {
    const code = data?.error?.code ?? "error";
    const message = data?.error?.message ?? res.statusText;
    throw new ApiError(res.status, code, message);
  }
  return data as T;
}

export const api = {
  get: <T>(path: string) => request<T>("GET", path),
  post: <T>(path: string, body?: unknown) => request<T>("POST", path, body),
  patch: <T>(path: string, body?: unknown) => request<T>("PATCH", path, body),
  put: <T>(path: string, body?: unknown) => request<T>("PUT", path, body),
  del: <T>(path: string) => request<T>("DELETE", path),
};
