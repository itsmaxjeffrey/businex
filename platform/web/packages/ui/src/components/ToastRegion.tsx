import { For, type Component } from "solid-js";

export interface ToastItem {
  id: string;
  tone: "success" | "error";
  text: string;
}

export interface ToastRegionProps {
  items: ToastItem[];
  onDismiss?: (id: string) => void;
}

/** Status region announced politely; error toasts stay until dismissed. */
export const ToastRegion: Component<ToastRegionProps> = (props) => {
  return (
    <div class="bx-toasts" role="status" aria-live="polite">
      <For each={props.items}>
        {(item) => (
          <div class={"bx-toast bx-toast--" + item.tone}>
            <span>{item.text}</span>
            <button
              type="button"
              class="bx-btn bx-btn--ghost bx-btn--sm"
              onClick={() => props.onDismiss?.(item.id)}
            >
              Dismiss
            </button>
          </div>
        )}
      </For>
    </div>
  );
};
