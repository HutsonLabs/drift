// Reconnect overlay (M7-3): "Reconnecting in N s… [Now] [Cancel]" over the dimmed last frame.
import type { SessionView_Serialize } from "../bindings";
import { h, mount } from "../dom";

/** Whole seconds left of a `nextInMs` backoff after `elapsedMs` (rounded up, never negative). */
export function secondsLeft(nextInMs: number, elapsedMs: number): number {
  return Math.max(0, Math.ceil((nextInMs - elapsedMs) / 1000));
}

/** The overlay headline for `seconds` left. */
export function reconnectMessage(seconds: number): string {
  return seconds > 0 ? `Reconnecting in ${seconds} s…` : "Reconnecting…";
}

/** Renders the overlay; `elapsedMs` is the time since the view arrived. */
export function renderReconnectOverlay(
  root: HTMLElement,
  view: SessionView_Serialize,
  elapsedMs: number,
  on: { now(): void; cancel(): void },
): void {
  const state = view.state;
  const attempt = state.state === "reconnecting" ? state.attempt : 1;
  const nextIn = state.state === "reconnecting" ? state.next_in : 0;
  const budget = view.max_attempts === null ? `Attempt ${attempt}` : `Attempt ${attempt} of ${view.max_attempts}`;
  const now = h("button", { type: "button", class: "primary", onclick: () => on.now() }, "Now");
  mount(
    root,
    h(
      "section",
      { class: "overlay-card reconnecting", "aria-labelledby": "reconnect-title" },
      h("h1", { id: "reconnect-title", class: "visually-hidden" }, `${view.profile_name} disconnected`),
      // The countdown is the live region; the profile and attempt budget are its description,
      // so VoiceOver announces which connection is retrying and how often (M9-4).
      h(
        "p",
        { class: "headline", role: "status", "aria-live": "polite", "aria-describedby": "reconnect-detail" },
        reconnectMessage(secondsLeft(nextIn, elapsedMs)),
      ),
      h("p", { class: "hint", id: "reconnect-detail" }, `${view.profile_name} · ${budget}`),
      h(
        "div",
        { class: "actions center" },
        h("button", { type: "button", onclick: () => on.cancel() }, "Cancel"),
        now,
      ),
    ),
  );
}
