import { createSignal } from "solid-js";
import type { ToastItem } from "@businex/ui";

let nextId = 1;
const [toasts, setToasts] = createSignal<ToastItem[]>([]);

export { toasts };

export function pushToast(tone: "success" | "error", text: string): void {
  const id = "toast-" + nextId++;
  setToasts((items) => items.concat({ id, tone, text }));
  if (tone === "success") {
    // Success confirmations clear themselves; error toasts stay until the
    // reader dismisses them. Auto-clear also keeps the fixed toast stack
    // from covering controls on small screens.
    window.setTimeout(() => dismissToast(id), 4000);
  }
}

export function dismissToast(id: string): void {
  setToasts((items) => items.filter((item) => item.id !== id));
}