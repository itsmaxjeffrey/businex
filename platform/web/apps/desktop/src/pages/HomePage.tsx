import { createSignal, Show, type Component } from "solid-js";
import { Button, Card, Dialog, Kpi, TextField } from "@businex/ui";
import { api } from "../api";
import { activeCompany, loadSession, messageOf, session } from "../state";
import { pushToast } from "../toasts";

export const HomePage: Component = () => {
  const [createOpen, setCreateOpen] = createSignal(false);
  const [companyName, setCompanyName] = createSignal("");
  const [inviteOpen, setInviteOpen] = createSignal(false);
  const [inviteToken, setInviteToken] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  async function createCompany(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api.createCompany({ name: companyName() });
      await loadSession();
      pushToast("success", "Company created.");
      setCreateOpen(false);
      setCompanyName("");
    } catch (err) {
      setError(messageOf(err));
    } finally {
      setBusy(false);
    }
  }

  async function acceptInvite(): Promise<void> {
    setBusy(true);
    setError(null);
    try {
      const result = await api.acceptInvitation({ token: inviteToken() });
      await loadSession();
      pushToast("success", "Joined company " + result.companyId + " as " + result.role + ".");
      setInviteOpen(false);
      setInviteToken("");
    } catch (err) {
      setError(messageOf(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div class="bx-stack">
      <div class="bx-page-head">
        <div>
          <h1 class="bx-page-title">Home</h1>
          <p class="bx-page-sub">Where the business stands right now.</p>
        </div>
        <Button onClick={() => setCreateOpen(true)}>Create company</Button>
      </div>

      <div class="bx-kpis">
        <Kpi label="Companies" value={session()?.companies.length ?? 0} />
        <Kpi label="Active role" value={activeCompany()?.role ?? "none"} />
      </div>

      <Card
        title="Your companies"
        description="Memberships attached to your account."
      >
        <Show
          when={(session()?.companies.length ?? 0) > 0}
          fallback={
            <p class="bx-field__hint">
              No companies yet. Create one, or accept an invitation from a
              teammate.
            </p>
          }
        >
          <ul class="bx-stack">
            <li class="bx-row">
              <span class="bx-code">{activeCompany()?.id ?? ""}</span>
              <span class="bx-badge bx-badge--brand">{activeCompany()?.role ?? ""}</span>
            </li>
          </ul>
        </Show>
      </Card>

      <Card title="Quick actions">
        <div class="bx-row">
          <Button variant="secondary" onClick={() => setInviteOpen(true)}>
            Accept invitation
          </Button>
        </div>
      </Card>

      <Dialog
        open={createOpen()}
        title="Create company"
        onClose={() => setCreateOpen(false)}
        footer={
          <Button type="submit" form="create-company" disabled={busy()}>
            Create
          </Button>
        }
      >
        <form
          id="create-company"
          class="bx-form"
          onSubmit={(event) => void createCompany(event)}
        >
          <TextField
            label="Company name"
            required
            value={companyName()}
            onInput={(event) => setCompanyName(event.currentTarget.value)}
          />
          <Show when={error()}>
            <p class="bx-field__error" role="alert">
              {error()}
            </p>
          </Show>
        </form>
      </Dialog>

      <Dialog
        open={inviteOpen()}
        title="Accept invitation"
        onClose={() => setInviteOpen(false)}
        footer={
          <Button onClick={() => void acceptInvite()} disabled={busy()}>
            Join company
          </Button>
        }
      >
        <TextField
          label="Invitation token"
          required
          hint="The single-use token your teammate shared with you."
          value={inviteToken()}
          onInput={(event) => setInviteToken(event.currentTarget.value)}
        />
        <Show when={error()}>
          <p class="bx-field__error" role="alert">
            {error()}
          </p>
        </Show>
      </Dialog>
    </div>
  );
};