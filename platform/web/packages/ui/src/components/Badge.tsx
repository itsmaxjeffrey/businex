import { splitProps, type Component, type JSX } from "solid-js";

export type BadgeTone = "neutral" | "brand" | "accent" | "success" | "danger";

export interface BadgeProps extends JSX.HTMLAttributes<HTMLSpanElement> {
  tone?: BadgeTone;
}

/** Status badge. The label carries the meaning; color only reinforces it. */
export const Badge: Component<BadgeProps> = (props) => {
  const [local, rest] = splitProps(props, ["tone", "children", "classList"]);
  return (
    <span
      {...rest}
      class={"bx-badge bx-badge--" + (local.tone ?? "neutral")}
      classList={local.classList}
    >
      {local.children}
    </span>
  );
};
