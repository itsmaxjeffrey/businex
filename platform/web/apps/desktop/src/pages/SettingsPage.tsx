import { Show, type Component } from "solid-js";
import { Button, Card } from "@businex/ui";
import { activeCompany, session, signOut } from "../state";

export const SettingsPage: Component = () => {
  return (
    <div class="bx-stack">
      <div class="bx-page-head">
        <div>
          <h1 class="bx-page-title">Settings</h1>
          <p class="bx-page-sub">Your account and this session.</p>
        </div>
      </div>

      <Card title="Your account">
        <dl class="bx-stack">
          <div class="bx-row">
            <dt class="bx-field__label">Name</dt>
            <dd>{session()?.user.name ?? ""}</dd>
          </div>
          <div class="bx-row">
            <dt class="bx-field__label">Email</dt>
            <dd class="bx-code">{session()?.user.email ?? ""}</dd>
          </div>
          <div class="bx-row">
            <dt class="bx-field__label">User id</dt>
            <dd class="bx-code">{session()?.user.id ?? ""}</dd>
          </div>
        </dl>
      </Card>

      <Show when={activeCompany()}>
        <Card title="Active company">
          <dl class="bx-stack">
            <div class="bx-row">
              <dt class="bx-field__label">Company id</dt>
              <dd class="bx-code">{activeCompany()?.id ?? ""}</dd>
            </div>
            <div class="bx-row">
              <dt class="bx-field__label">Your role</dt>
              <dd>{activeCompany()?.role ?? ""}</dd>
            </div>
          </dl>
        </Card>
      </Show>

      <Card title="Session" description="Sign out of the Businex desktop.">
        <Button variant="danger" onClick={() => void signOut()}>
          Sign out
        </Button>
      </Card>
    </div>
  );
};