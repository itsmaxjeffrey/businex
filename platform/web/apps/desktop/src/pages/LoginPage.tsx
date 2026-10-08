import { createSignal, Show, type Component } from "solid-js";
import { Button, Card, TextField } from "@businex/ui";
import { api } from "../api";
import { navigate } from "../router";
import { loadSession, messageOf } from "../state";
import { pushToast } from "../toasts";

export const LoginPage: Component = () => {
  const [email, setEmail] = createSignal("");
  const [password, setPassword] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api.login({ email: email(), password: password() });
      await loadSession();
      navigate("/");
    } catch (err) {
      setError(messageOf(err));
    } finally {
      setBusy(false);
    }
  }

  async function sso(): Promise<void> {
    try {
      const result = await api.oidcStart("/");
      window.location.href = result.authorization_url;
    } catch (err) {
      pushToast("error", messageOf(err));
    }
  }

  return (
    <Card
      title="Sign in"
      description="Use your work account to open the Businex desktop."
    >
      <form class="bx-form" onSubmit={(event) => void submit(event)}>
        <TextField
          label="Email"
          type="email"
          required
          autocomplete="email"
          value={email()}
          onInput={(event) => setEmail(event.currentTarget.value)}
        />
        <TextField
          label="Password"
          type="password"
          required
          autocomplete="current-password"
          value={password()}
          onInput={(event) => setPassword(event.currentTarget.value)}
        />
        <Show when={error()}>
          <p class="bx-field__error" role="alert">
            {error()}
          </p>
        </Show>
        <Button type="submit" block disabled={busy()}>
          Sign in
        </Button>
        <Button variant="secondary" block onClick={() => void sso()}>
          Sign in with SSO
        </Button>
        <p class="bx-field__hint">
          No account yet? <a href="#/register">Create an account</a>
        </p>
      </form>
    </Card>
  );
};