// M9-4 Red: VoiceOver labels. Every screen is audited for the things VoiceOver needs:
// an accessible name on every control, resolvable aria references, unique ids, hints that are
// actually announced (`aria-describedby`), decorative glyphs hidden, and a live picture that
// never puts a focus stop in front of the remote desktop.
import { describe, expect, test } from "bun:test";
import { renderCertificatePrompt } from "../src/views/certificate";
import { renderConnecting } from "../src/views/connecting";
import { renderError } from "../src/views/error";
import { renderGreeterHint } from "../src/views/greeter";
import { formModel, renderProfileForm } from "../src/views/profileForm";
import { profileList } from "../src/views/profileList";
import { renderReconnectOverlay } from "../src/views/reconnect";
import { renderStatsHud } from "../src/views/stats";
import { renderTabStrip } from "../src/views/tabStrip";
import { certPrompt, entry, LNP_EXPLANATION, profile, sessionView, stats, text } from "./helpers";

const FOCUSABLE = "button, input, select, textarea, summary, a[href]";

function root(): HTMLElement {
  const r = document.createElement("main");
  document.body.replaceChildren(r);
  return r;
}

/** The label text associated with a form control, `<label for>` or an ancestor `<label>`. */
function labelFor(el: Element): string {
  const id = el.getAttribute("id");
  const explicit = id ? el.ownerDocument.querySelector(`label[for="${id}"]`) : null;
  const label = explicit ?? el.closest("label");
  if (!label) return "";
  const clone = label.cloneNode(true) as HTMLElement;
  for (const c of Array.from(clone.querySelectorAll("input,select,textarea,.hint"))) c.remove();
  return text(clone);
}

/** The accessible name of `el`, in the order VoiceOver resolves it. */
export function accessibleName(el: Element): string {
  const aria = el.getAttribute("aria-label");
  if (aria?.trim()) return aria.trim();
  const ids = (el.getAttribute("aria-labelledby") ?? "").split(/\s+/).filter(Boolean);
  if (ids.length > 0) {
    return ids
      .map((id) => text(el.ownerDocument.querySelector(`[id="${id}"]`)))
      .filter(Boolean)
      .join(" ");
  }
  const named = labelFor(el);
  if (named) return named;
  if (el.matches("input,select,textarea")) return "";
  return text(el);
}

/** Everything wrong with `root`, as messages. Empty means the screen is announceable. */
export function auditAccessibility(root: ParentNode): string[] {
  const problems: string[] = [];
  const describe = (el: Element) => `<${el.tagName.toLowerCase()}${el.className ? `.${String(el.className).split(" ")[0]}` : ""}>`;

  // Every control VoiceOver can land on says what it is.
  for (const el of Array.from(root.querySelectorAll(FOCUSABLE))) {
    if (el.getAttribute("aria-hidden") === "true") continue;
    if (el.matches('input[type="hidden"]')) continue;
    if (!accessibleName(el)) problems.push(`${describe(el)} has no accessible name`);
  }

  // Every aria reference resolves, or VoiceOver announces nothing at all.
  for (const attr of ["aria-labelledby", "aria-describedby", "aria-controls"]) {
    for (const el of Array.from(root.querySelectorAll(`[${attr}]`))) {
      for (const id of (el.getAttribute(attr) ?? "").split(/\s+/).filter(Boolean)) {
        if (!el.ownerDocument.querySelector(`[id="${id}"]`)) {
          problems.push(`${describe(el)} ${attr}="${id}" points at nothing`);
        }
      }
    }
  }

  // Duplicate ids make those references ambiguous.
  const seen = new Set<string>();
  for (const el of Array.from(root.querySelectorAll("[id]"))) {
    const id = el.getAttribute("id") ?? "";
    if (seen.has(id)) problems.push(`duplicate id "${id}"`);
    seen.add(id);
  }

  // A hint nobody points at is invisible to VoiceOver: it is never read with its control.
  for (const hint of Array.from(root.querySelectorAll(".hint"))) {
    const id = hint.getAttribute("id");
    const referenced =
      id !== null && hint.ownerDocument.querySelector(`[aria-describedby~="${id}"], [aria-labelledby~="${id}"]`) !== null;
    if (!referenced) problems.push(`${describe(hint)} "${text(hint).slice(0, 40)}…" is not referenced by aria-describedby`);
  }

  // Ornamental glyphs must not be spoken.
  for (const icon of Array.from(root.querySelectorAll(".dialog-icon, .spinner"))) {
    const hidden = icon.getAttribute("aria-hidden") === "true" || icon.getAttribute("role") === "progressbar";
    if (!hidden) problems.push(`${describe(icon)} is decorative but not aria-hidden`);
  }
  return problems;
}

describe("accessible names", () => {
  test("the profile form announces every control and its hints", () => {
    const r = root();
    renderProfileForm(r, formModel(profile("remote-login"), { isNew: true, hasRdpPassword: false, hasLinuxPassword: true }), {
      change: () => {},
      save: () => {},
      cancel: () => {},
      remove: () => {},
      forgetCertificate: () => {},
    });
    expect(auditAccessibility(r)).toEqual([]);
  });

  test("every connection mode's form is announceable", () => {
    for (const mode of ["remote-login", "headless", "desktop-sharing"] as const) {
      const r = root();
      const p = profile(mode, { cert_pin: "aa:bb:cc" });
      renderProfileForm(r, formModel(p, { isNew: false, hasRdpPassword: true, hasLinuxPassword: false }), {
        change: () => {},
        save: () => {},
        cancel: () => {},
        remove: () => {},
        forgetCertificate: () => {},
      });
      expect(auditAccessibility(r)).toEqual([]);
    }
  });

  test("the saved-connections sidebar is a named landmark", () => {
    const r = root();
    const e = entry(profile("headless"));
    r.append(profileList([e], e.profile.id, { select: () => {}, connect: () => {}, create: () => {} }));
    expect(auditAccessibility(r)).toEqual([]);
    const nav = r.querySelector("nav");
    expect(accessibleName(nav as Element)).toBe("Saved connections");
    expect(accessibleName(r.querySelector("button.connect") as Element)).toBe("Connect to Homelab");
  });

  test("the certificate prompt announces how to verify the fingerprint", () => {
    const r = root();
    renderCertificatePrompt(r, certPrompt(), "Homelab", { trust: () => {}, cancel: () => {} });
    expect(auditAccessibility(r)).toEqual([]);
    const dialog = r.querySelector("[role=alertdialog]") as Element;
    const described = (dialog.getAttribute("aria-describedby") ?? "")
      .split(/\s+/)
      .map((id) => text(r.querySelector(`[id="${id}"]`)))
      .join(" ");
    expect(described).toContain("grdctl");
  });

  test("the connecting, greeter and error screens are announceable", () => {
    for (const render of [
      (r: HTMLElement) => renderConnecting(r, sessionView("connecting", { state: "connecting", leg: 2, stage: "tls" }), { cancel: () => {} }),
      (r: HTMLElement) => renderGreeterHint(r, sessionView("greeter-hint", { state: "awaiting-greeter-login" }, { resuming: true })),
      (r: HTMLElement) => renderError(r, LNP_EXPLANATION, "Homelab", () => {}),
    ]) {
      const r = root();
      render(r);
      expect(auditAccessibility(r)).toEqual([]);
    }
  });

  test("the reconnect overlay announces the attempt it is on", () => {
    const r = root();
    renderReconnectOverlay(r, sessionView("reconnecting", { state: "reconnecting", attempt: 3, next_in: 4000, reason: { kind: "network" } }), 1000, {
      now: () => {},
      cancel: () => {},
    });
    expect(auditAccessibility(r)).toEqual([]);
    const live = r.querySelector("[aria-live]") as Element;
    const described = (live.getAttribute("aria-describedby") ?? "")
      .split(/\s+/)
      .map((id) => text(r.querySelector(`[id="${id}"]`)))
      .join(" ");
    expect(described).toContain("Attempt 3 of 20");
  });

  test("the statistics HUD is a status, never a focus stop in front of the picture", () => {
    const r = root();
    renderStatsHud(r, stats());
    expect(auditAccessibility(r)).toEqual([]);
    expect(r.querySelectorAll(FOCUSABLE).length).toBe(0);
    const hud = r.querySelector(".hud") as Element;
    expect(hud.getAttribute("role")).toBe("status");
    expect(accessibleName(hud)).toBe("Session statistics");
  });

  test("the tab strip is a named tab list whose tabs announce their state (UI-tabs)", () => {
    const r = root();
    renderTabStrip(
      r,
      {
        tabs: [
          { id: "session-0", kind: "session", title: "Homelab", mode: "remote-login", status: "reconnecting", profile_id: null, hint: null },
          { id: "session-1", kind: "manager", title: "Connections", mode: null, status: "idle", profile_id: null, hint: null },
        ],
        active: "session-1",
        live_profiles: [],
      },
      { select: () => {}, close: () => {}, newTab: () => {}, focus: () => {} },
    );
    expect(auditAccessibility(r)).toEqual([]);
    expect(accessibleName(r.querySelector("nav") as Element)).toBe("Tabs");
    expect(accessibleName(r.querySelector("[role=tablist]") as Element)).toBe("Open tabs");
    // The dot is colour only; VoiceOver hears the state in the tab's description.
    const tab = r.querySelector("[role=tab]") as Element;
    const described = text(r.querySelector(`[id="${tab.getAttribute("aria-describedby")}"]`));
    expect(described).toBe("Reconnecting");
    for (const glyph of Array.from(r.querySelectorAll(".glyph, .spin, .status"))) {
      expect(glyph.getAttribute("aria-hidden")).toBe("true");
    }
  });
});
