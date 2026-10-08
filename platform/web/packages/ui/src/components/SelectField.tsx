import { createUniqueId, For, Show, splitProps, type Component, type JSX } from "solid-js";

export interface SelectOption {
  value: string;
  label: string;
}

export interface SelectFieldProps extends JSX.SelectHTMLAttributes<HTMLSelectElement> {
  label: string;
  options: SelectOption[];
  hint?: string;
  error?: string;
}

export const SelectField: Component<SelectFieldProps> = (props) => {
  const [local, rest] = splitProps(props, ["label", "options", "hint", "error", "id"]);
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
      <select
        {...rest}
        id={id()}
        class="bx-select"
        aria-invalid={local.error ? "true" : undefined}
        aria-describedby={describedBy()}
      >
        <For each={local.options}>
          {(option) => <option value={option.value}>{option.label}</option>}
        </For>
      </select>
      <Show when={local.error}>
        <p class="bx-field__error" id={errorId()} role="alert">{local.error}</p>
      </Show>
    </div>
  );
};
