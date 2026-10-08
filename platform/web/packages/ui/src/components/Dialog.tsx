import { createEffect, createUniqueId, Show, type Component, type JSX } from "solid-js";

export interface DialogProps {
  open: boolean;
  title: string;
  onClose: () => void;
  children?: JSX.Element;
  footer?: JSX.Element;
}

/** Native dialog element: Escape and backdrop dismissal come from the
 *  platform and focus stays trapped for free. One focused decision per
 *  dialog. Falls back to the open attribute where showModal is unavailable. */
export const Dialog: Component<DialogProps> = (props) => {
  let ref: HTMLDialogElement | undefined;
  const titleId = createUniqueId();
  createEffect(() => {
    if (!ref) return;
    if (props.open && !ref.open) {
      if (typeof ref.showModal === "function") ref.showModal();
      else ref.setAttribute("open", "");
    }
    if (!props.open && ref.open) {
      if (typeof ref.close === "function") ref.close();
      else ref.removeAttribute("open");
    }
  });
  return (
    <dialog
      ref={ref}
      class="bx-dialog"
      aria-labelledby={titleId}
      onCancel={(event) => {
        event.preventDefault();
        props.onClose();
      }}
    >
      <h2 class="bx-dialog__title" id={titleId}>
        {props.title}
      </h2>
      {props.children}
      <div class="bx-dialog__actions">
        <Show
          when={props.footer}
          fallback={
            <button
              type="button"
              class="bx-btn bx-btn--secondary"
              onClick={() => props.onClose()}
            >
              Close
            </button>
          }
        >
          {props.footer}
        </Show>
      </div>
    </dialog>
  );
};
