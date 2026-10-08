import { createSignal, Show, type Component } from "solid-js";
import { Button, Card, TextField } from "@businex/ui";
import { api } from "../api";
import { navigate } from "../router";
import { loadSession, messageOf } from "../state";

export const RegisterPage: Component = () => {
  const [email, setEmail] = createSignal("");
  const [name, setName] = createSignal("");
  const [password, setPassword] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api.register({
        email: email(),
        password: password(),
        name: name().length > 0 ? name() : undefined
      });
      await loadSession();
      navigate("/");
    } catch (err) {
      setError(messageOf(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card title="Create your account" description="Start with your work e-mail.">
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
          label="Name"
          autocomplete="name"
          hint="Shown to your teammates. Optional."
          value={name()}
          onInput={(event) => setName(event.currentTarget.value)}
        />
        <TextField
          label="Password"
          type="password"
          required
          autocomplete="new-password"
          hint="At least 12 characters."
          value={password()}
          onInput={(event) => setPassword(event.currentTarget.value)}
        />
        <Show when={error()}>
          <p class="bx-field__error" role="alert">
            {error()}
          </p>
        </Show>
        <Button type="submit" block disabled={busy()}>
          Create account
        </Button>
        <p class="bx-field__hint">
          Registration may be disabled by your administrator. If it is, ask
          for an invitation instead. <a href="#/login">Back to sign in</a>
        </p>
      </form>
    </Card>
  );
};