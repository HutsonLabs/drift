// Certificate prompt (M1-6, TOFU): shows the SHA-256 fingerprint in the exact format that
// `grdctl status` prints ("TLS fingerprint: f3:e7:a2:…") so users can compare the two.
import type { CertificatePrompt } from "../bindings";
import { h, mount } from "../dom";
import { icon } from "../icons";

/** Answers to the prompt. */
export interface CertificateIntents {
  /** Trust it; `pin` = remember it for this profile. */
  trust(pin: boolean): void;
  cancel(): void;
}

/** Renders the prompt as an alert dialog. */
export function renderCertificatePrompt(
  root: HTMLElement,
  prompt: CertificatePrompt,
  profileName: string,
  on: CertificateIntents,
): void {
  const redirect = prompt.subject === "redirect-target";
  const address = `${prompt.host}:${prompt.port}`;
  const remember = h("input", { type: "checkbox", role: "switch", id: "remember-cert", checked: true });
  const connect = h("button", { type: "button", class: "primary", onclick: () => on.trust(remember.checked) }, "Connect");

  const dialog = h(
    "section",
    // Both the explanation and the "how to compare it" hint are read out: without the second
    // one a VoiceOver user is asked to verify a fingerprint with no way to check it (M9-4).
    { class: "sheet dialog certificate", role: "alertdialog", "aria-labelledby": "cert-title", "aria-describedby": "cert-desc cert-hint" },
    h("div", { class: "dialog-icon shield", "aria-hidden": "true" }, icon("shield")),
    h("h1", { id: "cert-title" }, redirect ? `Verify the login server for “${profileName}”` : `Verify the identity of ${address}`),
    h(
      "p",
      { id: "cert-desc" },
      redirect
        ? `After the login screen, the host hands the connection over to a server at ${address}. Drift hasn’t seen its certificate before.`
        : `Drift hasn’t connected to ${address} (“${profileName}”) before. Make sure the fingerprint below matches the one on the host before you continue.`,
    ),
    h(
      "div",
      { class: "fp-card" },
      h("p", { class: "fingerprint-label", id: "fp-label" }, "SHA-256 fingerprint"),
      h("code", { class: "fingerprint", "aria-labelledby": "fp-label" }, prompt.fingerprint),
    ),
    h(
      "p",
      { class: "hint", id: "cert-hint" },
      "On the host, run ",
      h("code", {}, prompt.grdctl_command),
      " and compare the line “TLS fingerprint”.",
    ),
    h("label", { class: "check" }, remember, "Remember this certificate"),
    h(
      "div",
      { class: "actions split" },
      h("button", { type: "button", onclick: () => on.cancel() }, "Cancel"),
      connect,
    ),
  );
  mount(root, h("div", { class: "scrim" }, dialog));
  connect.focus();
}
