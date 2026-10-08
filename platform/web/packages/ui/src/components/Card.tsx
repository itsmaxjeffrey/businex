import { Show, splitProps, type Component, type JSX } from "solid-js";

export interface CardProps extends JSX.HTMLAttributes<HTMLElement> {
  title?: string;
  description?: string;
  flush?: boolean;
}

export const Card: Component<CardProps> = (props) => {
  const [local, rest] = splitProps(props, [
    "title",
    "description",
    "flush",
    "children",
    "classList"
  ]);
  return (
    <section
      {...rest}
      class="bx-card"
      classList={{ "bx-card--flush": local.flush ?? false, ...(local.classList ?? {}) }}
    >
      <Show when={local.title}>
        <h2 class="bx-card__title">{local.title}</h2>
      </Show>
      <Show when={local.description}>
        <p class="bx-card__desc">{local.description}</p>
      </Show>
      {local.children}
    </section>
  );
};
