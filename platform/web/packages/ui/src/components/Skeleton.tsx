import { type Component } from "solid-js";

export interface SkeletonProps {
  width?: string;
  height?: string;
}

/** Loading placeholder shaped like the content it replaces. Decorative by
 *  design; the surrounding view announces loading through a live region. */
export const Skeleton: Component<SkeletonProps> = (props) => {
  return (
    <div
      class="bx-skeleton"
      style={{ width: props.width ?? "100%", height: props.height ?? "1em" }}
      aria-hidden="true"
    />
  );
};
