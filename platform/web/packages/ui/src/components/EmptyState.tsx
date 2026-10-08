import { Show, type Component, type JSX } from "solid-js";

export interface EmptyStateProps {
  title: string;
  body: string;
  action?: JSX.Element;
}

/** Explains what appears here and offers one next action (component-taste: empty states). */
export const EmptyState: Component<EmptyStateProps> = (props) => {
  return (
    <div class="bx-empty">
      <h3 class="bx-empty__title">{props.title}</h3>
      <p class="bx-empty__body">{props.body}</p>
      <Show when={props.action}>{props.action}</Show>
    </div>
  );
};
