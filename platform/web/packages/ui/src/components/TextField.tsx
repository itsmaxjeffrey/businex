import { createUniqueId, Show, splitProps, type Component, type JSX } from "solid-js";

export interface TextFieldProps extends JSX.InputHTMLAttributes<HTMLInputElement> {
  label: string;
  hint?: string;
  error?: string;
}

/** Label above the input (never placeholder-as-label); errors sit next to the field
 *  and are announced through role=alert. */
export const TextField: Component<TextFieldProps> = (props) => {
  const [local, rest] = splitProps(props, ["label", "hint", "error", "id"]);
  const autoId = createUniqueId();
  const id = () => local.id ?? autoId;
  const hintId = () => id() + "-hint";
  const errorId = () => id() + "-error";
  const describedBy = () =>
    [local.hint ? hintId() : "", local.error ? errorId() : ""]
      .filter((part) => part.length > 0)
      .join(" ") || undefined;
  return (
    <div class="bx-field">
      <label class="bx-field__label" for={id()}>
        {local.label}
        <Show when={rest.required}>
          <span class="bx-field__required"> (required)</span>
        </Show>
      </label>
      <Show when={local.hint}>
        <p class="bx-field__hint" id={hintId()}>{local.hint}</p>
      </Show>
      <input
        {...rest}
        id={id()}
        class="bx-input"
        aria-invalid={local.error ? "true" : undefined}
        aria-describedby={describedBy()}
      />
      <Show when={local.error}>
        <p class="bx-field__error" id={errorId()} role="alert">{local.error}</p>
      </Show>
    </div>
  );
};
