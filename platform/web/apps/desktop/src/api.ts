/**
 * Typed client for the Businex platform API (Rust/Axum).
 *
 * Sessions are HttpOnly cookies, so every request includes credentials and the
 * dev server proxies /api to the API for same-origin cookies.
 */

export interface User {
  id: string;
  email: string;
  name: string;
}

export interface CompanyRef {
  id: string;
  role: string;
}

export interface Member {
  user_id: string;
  email: string;
  name: string;
  role: string;
}

export interface MemberUpdateResult {
  userId: string;
  role: string;
}

export interface InvitationResult {
  id: string;
  token: string;
  expiresAt: string;
}

export interface AcceptResult {
  companyId: string;
  role: string;
}

export interface CompanyResult {
  id: string;
  name: string;
  slug: string;
}

export interface OidcStartResult {
  authorization_url: string;
}

export class ApiError extends Error {
  status: number;

  constructor(status: number, message: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
  }
}

function extractError(data: unknown): string | undefined {
  if (data && typeof data === "object" && "error" in data) {
    const message = (data as Record<string, unknown>).error;
    if (typeof message === "string" && message.length > 0) return message;
  }
  return undefined;
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const response = await fetch(path, {
    method,
    credentials: "include",
    headers: body === undefined ? undefined : { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body)
  });
  const text = await response.text();
  let data: unknown = null;
  if (text.length > 0) {
    try {
      data = JSON.parse(text);
    } catch {
      data = null;
    }
  }
  if (!response.ok) {
    const message = extractError(data) ?? "Request failed with status " + response.status;
    throw new ApiError(response.status, message);
  }
  return data as T;
}

export const api = {
  me: () => request<{ user: User; companies: CompanyRef[] }>("GET", "/api/auth/me"),

  login: (input: { email: string; password: string }) =>
    request<{ user: User }>("POST", "/api/auth/login", input),

  register: (input: { email: string; password: string; name?: string }) =>
    request<{ user: User }>("POST", "/api/auth/register", input),

  logout: () => request<unknown>("POST", "/api/auth/logout", {}),

  oidcStart: (returnTo: string) =>
    request<OidcStartResult>("GET", "/api/auth/oidc/start?return_to=" + encodeURIComponent(returnTo)),

  createCompany: (input: { name: string }) =>
    request<CompanyResult>("POST", "/api/companies", input),

  listMembers: (companyId: string) =>
    request<{ members: Member[] }>(
      "GET",
      "/api/companies/" + encodeURIComponent(companyId) + "/members"
    ),

  updateMember: (companyId: string, userId: string, input: { role: string }) =>
    request<MemberUpdateResult>(
      "PATCH",
      "/api/companies/" + encodeURIComponent(companyId) + "/members/" + encodeURIComponent(userId),
      input
    ),

  removeMember: (companyId: string, userId: string) =>
    request<unknown>(
      "DELETE",
      "/api/companies/" + encodeURIComponent(companyId) + "/members/" + encodeURIComponent(userId)
    ),

  createInvitation: (companyId: string, input: { email: string; role: string }) =>
    request<InvitationResult>(
      "POST",
      "/api/companies/" + encodeURIComponent(companyId) + "/invitations",
      input
    ),

  acceptInvitation: (input: { token: string }) =>
    request<AcceptResult>("POST", "/api/invitations/accept", input)
};
