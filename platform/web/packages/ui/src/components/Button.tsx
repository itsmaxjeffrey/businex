import { splitProps, type Component, type JSX } from "solid-js";

export type ButtonVariant = "primary" | "secondary" | "danger" | "ghost";
export type ButtonSize = "sm" | "md";

export interface ButtonProps extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  block?: boolean;
}

export const Button: Component<ButtonProps> = (props) => {
  const [local, rest] = splitProps(props, ["variant", "size", "block", "children", "classList"]);
  const cls = () =>
    "bx-btn bx-btn--" +
    (local.variant ?? "primary") +
    (local.size === "sm" ? " bx-btn--sm" : "") +
    (local.block ? " bx-btn--block" : "");
  return (
    <button {...rest} type={rest.type ?? "button"} class={cls()} classList={local.classList}>
      {local.children}
    </button>
  );
};
