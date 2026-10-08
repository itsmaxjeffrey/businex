import { createSignal, For, onCleanup, onMount, Show, type Component } from "solid-js";
import { AppShell, Button, Skeleton, ToastRegion, type NavItem } from "@businex/ui";
import {
  activeCompanyId,
  loadSession,
  session,
  sessionLoading,
  setActiveCompanyId,
  signOut
} from "./state";
import { currentPath } from "./router";
import { dismissToast, toasts } from "./toasts";
import { HomePage } from "./pages/HomePage";
import { LoginPage } from "./pages/LoginPage";
import { RegisterPage } from "./pages/RegisterPage";
import { SettingsPage } from "./pages/SettingsPage";
import { TeamPage } from "./pages/TeamPage";

const NAV: NavItem[] = [
  { label: "Home", href: "#/" },
  { label: "Team", href: "#/team" },
  { label: "Settings", href: "#/settings" }
];

export const App: Component = () => {
  const [path, setPath] = createSignal(currentPath());

  onMount(() => {
    const sync = (): void => {
      setPath(currentPath());
    };
    window.addEventListener("hashchange", sync);
    onCleanup(() => window.removeEventListener("hashchange", sync));
    void loadSession();
  });

  return (
    <Show
      when={!sessionLoading()}
      fallback={
        <main class="bx-content" data-testid="boot">
          <Skeleton width="100%" height="24px" />
          <Skeleton width="100%" height="240px" />
        </main>
      }
    >
      <Show
        when={session()}
        fallback={
          <main class="bx-content">
            <Show when={path() === "/register"} fallback={<LoginPage />}>
              <RegisterPage />
            </Show>
          </main>
        }
      >
        <AppShell
          brand="Businex"
          nav={NAV}
          current={"#" + path()}
          topbar={
            <div class="bx-row">
              <select
                class="bx-select bx-select--inline"
                aria-label="Active company"
                value={activeCompanyId() ?? ""}
                onChange={(event) => setActiveCompanyId(event.currentTarget.value)}
              >
                <For each={session()?.companies ?? []}>
                  {(company) => (
                    <option value={company.id}>
                      {company.role + " - " + company.id.slice(0, 8)}
                    </option>
                  )}
                </For>
              </select>
              <span>{session()?.user.name ?? ""}</span>
              <Button variant="ghost" size="sm" onClick={() => void signOut()}>
                Sign out
              </Button>
            </div>
          }
        >
          <Show
            when={path() === "/team"}
            fallback={
              <Show when={path() === "/settings"} fallback={<HomePage />}>
                <SettingsPage />
              </Show>
            }
          >
            <TeamPage />
          </Show>
        </AppShell>
      </Show>
      <ToastRegion items={toasts()} onDismiss={dismissToast} />
    </Show>
  );
};