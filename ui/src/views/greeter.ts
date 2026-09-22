// Greeter-wait hint (M3-2, M7-3): a non-modal banner while the GNOME login screen is live.
import type { SessionView_Serialize } from "../bindings";
import { h, mount } from "../dom";

/** Renders the hint banner. */
export function renderGreeterHint(root: HTMLElement, view: SessionView_Serialize): void {
  const headline = view.resuming ? "Session is still running — log in to resume" : "Log in to start your session";
  const who = view.linux_username
    ? `Log in as “${view.linux_username}” on the GNOME login screen.`
    : "Log in on the GNOME login screen.";
  mount(
    root,
    h(
      "section",
      { class: "banner greeter", role: "status", "aria-live": "polite" },
      h("strong", {}, headline),
      h("span", {}, who),
    ),
  );
}
