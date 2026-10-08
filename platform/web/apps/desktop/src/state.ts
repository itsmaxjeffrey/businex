import { createSignal } from "solid-js";
import { api, ApiError, type CompanyRef, type User } from "./api";

export interface SessionState {
  user: User;
  companies: CompanyRef[];
}

const [session, setSession] = createSignal<SessionState | null>(null);
const [sessionLoading, setSessionLoading] = createSignal(true);
const [activeCompanyId, setActiveCompanyId] = createSignal<string | null>(null);

export { session, sessionLoading, activeCompanyId, setActiveCompanyId };

export function activeCompany(): CompanyRef | null {
  const id = activeCompanyId();
  return session()?.companies.find((company) => company.id === id) ?? null;
}

export function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : "Something went wrong. Try again.";
}

export async function loadSession(): Promise<void> {
  setSessionLoading(true);
  try {
    const me = await api.me();
    setSession({ user: me.user, companies: me.companies });
    setActiveCompanyId((current) => current ?? me.companies[0]?.id ?? null);
  } catch (error) {
    // 401 means signed out; anything else also leaves us at the sign-in
    // screen with the session cleared so the user can retry deliberately.
    setSession(null);
    if (!(error instanceof ApiError && error.status === 401)) {
      // Non-auth failures still resolve the boot state; pages surface errors.
    }
  } finally {
    setSessionLoading(false);
  }
}

export async function signOut(): Promise<void> {
  await api.logout();
  setSession(null);
  setActiveCompanyId(null);
}
