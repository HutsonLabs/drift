// A confirmation sheet over the Connections page ("Delete “Homelab”?"). Cancel has the keyboard,
// so Return never destroys anything by accident; Esc cancels.
import { h, mount } from "../dom";
import { icon } from "../icons";

/** What the confirmation says and does. */
export interface Confirmation {
  title: string;
  message: string;
  /** The destructive button's label. */
  action: string;
  confirm(): void;
  cancel(): void;
}

/** Renders `c` into `root`, replacing its contents. */
export function renderConfirm(root: HTMLElement, c: Confirmation): void {
  const cancel = h("button", { type: "button", onclick: () => c.cancel() }, "Cancel");
  const dialog = h(
    "section",
    { class: "sheet dialog confirm", role: "alertdialog", "aria-modal": "true", "aria-labelledby": "confirm-title", "aria-describedby": "confirm-desc" },
    h("div", { class: "dialog-icon warn", "aria-hidden": "true" }, icon("warn")),
    h("h2", { id: "confirm-title" }, c.title),
    h("p", { id: "confirm-desc" }, c.message),
    h("div", { class: "actions split" }, cancel, h("button", { type: "button", class: "destructive", onclick: () => c.confirm() }, c.action)),
  );
  dialog.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      c.cancel();
    }
  });
  mount(root, h("div", { class: "scrim confirm-scrim" }, dialog));
  cancel.focus();
}
