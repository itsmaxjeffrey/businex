import { createEffect, createSignal, Show, type Component } from "solid-js";
import {
  Badge,
  Button,
  DataTable,
  Dialog,
  SelectField,
  TextField,
  type Cell,
  type Column,
  type Row
} from "@businex/ui";
import { api, type Member } from "../api";
import { activeCompanyId, messageOf } from "../state";
import { pushToast } from "../toasts";

const ROLE_OPTIONS = [
  { value: "admin", label: "Admin" },
  { value: "member", label: "Member" },
  { value: "viewer", label: "Viewer" }
];

function toneFor(role: string): "brand" | "accent" | "neutral" {
  if (role === "owner") return "brand";
  if (role === "admin") return "accent";
  return "neutral";
}

export const TeamPage: Component = () => {
  const [members, setMembers] = createSignal<Member[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [inviteOpen, setInviteOpen] = createSignal(false);
  const [inviteEmail, setInviteEmail] = createSignal("");
  const [inviteRole, setInviteRole] = createSignal("member");
  const [issued, setIssued] = createSignal<{ token: string; expiresAt: string } | null>(
    null
  );
  const [removing, setRemoving] = createSignal<Member | null>(null);

  async function refresh(id: string): Promise<void> {
    setLoading(true);
    setError(null);
    try {
      const result = await api.listMembers(id);
      setMembers(result.members);
    } catch (err) {
      setError(messageOf(err));
    } finally {
      setLoading(false);
    }
  }

  createEffect(() => {
    const id = activeCompanyId();
    if (id) void refresh(id);
    else setMembers([]);
  });

  async function changeRole(member: Member, role: string): Promise<void> {
    const id = activeCompanyId();
    if (!id) return;
    setBusy(true);
    try {
      await api.updateMember(id, member.user_id, { role });
      pushToast("success", member.name + " is now " + role + ".");
      await refresh(id);
    } catch (err) {
      pushToast("error", messageOf(err));
    } finally {
      setBusy(false);
    }
  }

  async function sendInvite(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    const id = activeCompanyId();
    if (!id) return;
    setBusy(true);
    setError(null);
    try {
      const result = await api.createInvitation(id, {
        email: inviteEmail(),
        role: inviteRole()
      });
      setIssued({ token: result.token, expiresAt: result.expiresAt });
    } catch (err) {
      setError(messageOf(err));
    } finally {
      setBusy(false);
    }
  }

  async function confirmRemove(): Promise<void> {
    const id = activeCompanyId();
    const member = removing();
    if (!id || !member) return;
    setBusy(true);
    try {
      await api.removeMember(id, member.user_id);
      pushToast("success", member.name + " was removed.");
      setRemoving(null);
      await refresh(id);
    } catch (err) {
      pushToast("error", messageOf(err));
    } finally {
      setBusy(false);
    }
  }

  const columns: Column[] = [
    { key: "name", header: "Name" },
    { key: "email", header: "Email", code: true },
    { key: "role", header: "Role" },
    { key: "actions", header: "Actions" }
  ];

  function rows(): Row[] {
    return members().map((member) => {
      const actions: Cell =
        member.role === "owner" ? (
          <span class="bx-field__hint">Transferred by owner only</span>
        ) : (
          <span class="bx-row">
            <select
              class="bx-select bx-select--inline"
              aria-label={"Role for " + member.name}
              value={member.role}
              onChange={(event) => void changeRole(member, event.currentTarget.value)}
            >
              <option value="admin">Admin</option>
              <option value="member">Member</option>
              <option value="viewer">Viewer</option>
            </select>
            <Button
              variant="danger"
              size="sm"
              onClick={() => setRemoving(member)}
            >
              Remove
            </Button>
          </span>
        );
      return {
        id: member.user_id,
        cells: {
          name: member.name,
          email: member.email,
          role: <Badge tone={toneFor(member.role)}>{member.role}</Badge>,
          actions,
        }
      };
    });
  }

  return (
    <div class="bx-stack">
      <div class="bx-page-head">
        <div>
          <h1 class="bx-page-title">Team</h1>
          <p class="bx-page-sub">
            Members, roles and invitations for the active company.
          </p>
        </div>
        <Button
          onClick={() => {
            setIssued(null);
            setInviteOpen(true);
          }}
          disabled={!activeCompanyId()}
        >
          Invite member
        </Button>
      </div>

      <Show when={error() && !inviteOpen()}>
        <p class="bx-field__error" role="alert">
          {error()}
        </p>
      </Show>

      <Show
        when={activeCompanyId()}
        fallback={
          <p class="bx-field__hint">
            Create or join a company first; the team list follows the active
            company.
          </p>
        }
      >
        <DataTable
          caption="Members of the active company"
          columns={columns}
          rows={rows()}
          emptyTitle={loading() ? "Loading members" : "No members yet"}
          emptyBody="Invite a teammate to see them listed here."
        />
      </Show>

      <Dialog
        open={inviteOpen()}
        title="Invite a teammate"
        onClose={() => setInviteOpen(false)}
        footer={
          <Show
            when={!issued()}
            fallback={<Button onClick={() => setInviteOpen(false)}>Done</Button>}
          >
            <Button
              type="submit"
              form="invite-member"
              disabled={busy()}
            >
              Send invite
            </Button>
          </Show>
        }
      >
        <Show
          when={!issued()}
          fallback={
            <div class="bx-stack">
              <p>
                Share this single-use token with your teammate. It expires at
                {" "}
                <span class="bx-code">{issued()?.expiresAt ?? ""}</span>.
              </p>
              <p class="bx-code">{issued()?.token ?? ""}</p>
            </div>
          }
        >
          <form
            id="invite-member"
            class="bx-form"
            onSubmit={(event) => void sendInvite(event)}
          >
            <TextField
              label="Email"
              type="email"
              required
              value={inviteEmail()}
              onInput={(event) => setInviteEmail(event.currentTarget.value)}
            />
            <SelectField
              label="Role"
              options={ROLE_OPTIONS}
              value={inviteRole()}
              onChange={(event) => setInviteRole(event.currentTarget.value)}
            />
            <Show when={error()}>
              <p class="bx-field__error" role="alert">
                {error()}
              </p>
            </Show>
          </form>
        </Show>
      </Dialog>

      <Dialog
        open={removing() !== null}
        title="Remove member"
        onClose={() => setRemoving(null)}
        footer={
          <Button variant="danger" onClick={() => void confirmRemove()} disabled={busy()}>
            Remove
          </Button>
        }
      >
        <p>
          Remove <strong>{removing()?.name ?? ""}</strong> from this company? They
          lose access immediately; the audit trail keeps the record.
        </p>
      </Dialog>
    </div>
  );
};