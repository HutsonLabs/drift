// Error screen (M1-6, M9-3, M9-4): renders an ErrorExplanation from Rust (title, message,
// mode-specific next steps) with its actions. The Local Network Privacy case offers
// "Open Local Network Settings" (System Settings › Privacy & Security › Local Network).
import type { ErrorAction, ErrorExplanation } from "../bindings";
import { h, mount } from "../dom";
import { icon } from "../icons";

const LABELS: Record<ErrorAction, string> = {
  reconnect: "Reconnect",
  "edit-profile": "Edit Connection…",
  "open-local-network-settings": "Open Local Network Settings",
  close: "Close",
};

/** Renders `explanation`; `act` receives the chosen action. */
export function renderError(
  root: HTMLElement,
  explanation: ErrorExplanation,
  profileName: string,
  act: (action: ErrorAction) => void,
): void {
  const buttons = explanation.actions.map((a, i) =>
    h("button", { type: "button", class: i === 0 ? "primary" : null, onclick: () => act(a) }, LABELS[a]),
  );
  mount(
    root,
    h(
      "div",
      { class: "scrim" },
      h(
        "section",
        { class: "sheet error", role: "alert", "aria-labelledby": "error-title" },
        h("div", { class: "dialog-icon warn", "aria-hidden": "true" }, icon("warn")),
        h("h1", { id: "error-title" }, explanation.title),
        h("p", { class: "profile" }, profileName),
        h("p", {}, explanation.message),
        explanation.next_steps.length > 0 &&
          h(
            "section",
            { class: "next-steps", "aria-label": "What to do" },
            h("h2", {}, "What to do"),
            h("ol", {}, explanation.next_steps.map((s) => h("li", {}, s))),
          ),
        h("div", { class: "actions center" }, [...buttons].reverse()),
      ),
    ),
  );
  buttons[0]?.focus();
}
