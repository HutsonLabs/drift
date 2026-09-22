// Connecting progress (shown until the first picture arrives).
import type { ConnectStage, SessionView_Serialize } from "../bindings";
import { h, mount } from "../dom";

const STAGES: Record<ConnectStage, string> = {
  tcp: "Contacting the host…",
  tls: "Securing the connection…",
  nla: "Signing in…",
  rdstls: "Opening the GNOME login screen…",
  activation: "Starting the remote desktop…",
};

/** Renders the progress screen for a `connecting` state. */
export function renderConnecting(root: HTMLElement, view: SessionView_Serialize, on: { cancel(): void }): void {
  const state = view.state;
  const stage = state.state === "connecting" ? state.stage : "tcp";
  const leg = state.state === "connecting" ? state.leg : 1;
  const detail =
    view.mode === "remote-login" && leg >= 2 && stage !== "rdstls"
      ? `${STAGES[stage]} (opening the login screen)`
      : STAGES[stage];
  mount(
    root,
    h(
      "section",
      { class: "panel connecting", "aria-labelledby": "connecting-title" },
      h("div", { class: "spinner", role: "progressbar", "aria-label": "Connecting", "aria-busy": "true" }),
      h("h1", { id: "connecting-title" }, `Connecting to ${view.profile_name}…`),
      h("p", { role: "status", "aria-live": "polite" }, detail),
      h("div", { class: "actions center" }, h("button", { type: "button", onclick: () => on.cancel() }, "Cancel")),
    ),
  );
}
