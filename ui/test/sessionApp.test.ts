// M1-6 / M7-3 / UI-windows Red: a session window's page renders `SessionView.screen` and turns
// clicks into IPC intents. It has no profiles screen any more: Cancel and the error sheet's Close
// close the window, and Edit Connection… opens the edit sheet on the Connections window.
import { describe, expect, test } from "bun:test";
import type { CommandError } from "../src/bindings";
import { type SessionApi, SessionApp } from "../src/sessionApp";
import { button, certPrompt, flush, GRDCTL_FINGERPRINT, identity, LNP_EXPLANATION, sessionView, stats, text } from "./helpers";

type Result<T> = { status: "ok"; data: T } | { status: "error"; error: CommandError };
const ok = <T>(data: T): Promise<Result<T>> => Promise.resolve({ status: "ok", data });

const PROFILE_ID = "00000000-0000-0000-0000-00000000beef";

class FakeSessionApi {
  calls: unknown[][] = [];
  names(): string[] {
    return this.calls.map((c) => String(c[0]));
  }
  api(): SessionApi {
    const log = (...c: unknown[]) => this.calls.push(c);
    return {
      openLocalNetworkSettings: () => (log("openLocalNetworkSettings"), ok(null)),
      acceptCertificate: (fp: string, pin: boolean) => (log("acceptCertificate", fp, pin), ok(null)),
      rejectCertificate: () => (log("rejectCertificate"), ok(null)),
      reconnectNow: () => (log("reconnectNow"), ok(null)),
      cancelReconnect: () => (log("cancelReconnect"), ok(null)),
      closeSession: () => (log("closeSession"), ok(null)),
      showConnections: (edit: string | null) => (log("showConnections", edit), ok(null)),
      windowIdentity: () => (log("windowIdentity"), ok(identity({ profile_id: PROFILE_ID, name: "Share", mode: "desktop-sharing" }))),
    } as unknown as SessionApi;
  }
}

async function start(fake = new FakeSessionApi()) {
  const root = document.createElement("main");
  document.body.replaceChildren(root);
  const app = new SessionApp(root, fake.api());
  await app.start();
  await flush();
  return { root, app, fake };
}

const CONNECTING = { state: "connecting", leg: 1, stage: "tls" } as const;
const LIVE = { state: "connected", desktop: { width: 1280, height: 800 }, scale: 100 } as const;

describe("session window page", () => {
  test("certificate prompt → acceptCertificate(fingerprint, remember)", async () => {
    const { root, app, fake } = await start();
    app.onSessionView(sessionView("certificate", CONNECTING, { certificate: certPrompt() }));
    expect(document.body.dataset.screen).toBe("certificate");
    expect(text(root.querySelector(".fingerprint"))).toBe(GRDCTL_FINGERPRINT);
    button(root, "Connect").click();
    await flush();
    expect(fake.calls).toContainEqual(["acceptCertificate", GRDCTL_FINGERPRINT, true]);
  });

  test("certificate cancel → rejectCertificate", async () => {
    const { root, app, fake } = await start();
    app.onSessionView(sessionView("certificate", CONNECTING, { certificate: certPrompt() }));
    button(root, "Cancel").click();
    await flush();
    expect(fake.names()).toContain("rejectCertificate");
  });

  test("Cancel while connecting closes the window", async () => {
    const { app, root, fake } = await start();
    app.onSessionView(sessionView("connecting", CONNECTING));
    button(root, "Cancel").click();
    await flush();
    expect(fake.names()).toContain("closeSession");
  });

  test("reconnect overlay → reconnectNow / cancelReconnect", async () => {
    const { root, app, fake } = await start();
    app.onSessionView(sessionView("reconnecting", { state: "reconnecting", attempt: 1, next_in: 2000, reason: { kind: "network" } }));
    expect(document.body.dataset.screen).toBe("reconnecting");
    expect(text(root)).toContain("Reconnecting in 2 s…");
    button(root, "Now").click();
    button(root, "Cancel").click();
    await flush();
    expect(fake.names()).toEqual(expect.arrayContaining(["reconnectNow", "cancelReconnect"]));
    app.dispose();
  });

  test("greeter hint while the GDM login screen is live", async () => {
    const { root, app } = await start();
    app.onSessionView(sessionView("greeter-hint", { state: "awaiting-greeter-login" }, { resuming: true }));
    expect(document.body.dataset.screen).toBe("greeter-hint");
    expect(document.body.dataset.hud).toBe("banner");
    expect(text(root.querySelector(".banner"))).toContain("Session is still running — log in to resume");
  });

  test("Local Network error opens System Settings", async () => {
    const { root, app, fake } = await start();
    app.onSessionView(sessionView("error", { state: "failed", reason: { kind: "local-network-denied" } }, { explanation: LNP_EXPLANATION }));
    button(root, "Open Local Network Settings").click();
    await flush();
    expect(fake.names()).toContain("openLocalNetworkSettings");
  });

  test("error Reconnect retries; Close closes the window", async () => {
    const { root, app, fake } = await start();
    app.onSessionView(
      sessionView("error", { state: "disconnected", reason: { kind: "user-closed" } }, {
        explanation: { title: "Disconnected", message: "You closed the connection.", next_steps: [], actions: ["reconnect", "close"] },
      }),
    );
    button(root, "Reconnect").click();
    button(root, "Close").click();
    await flush();
    expect(fake.names()).toEqual(expect.arrayContaining(["reconnectNow", "closeSession"]));
  });

  test("Edit Connection… opens this profile's edit sheet on the Connections window", async () => {
    const { root, app, fake } = await start();
    app.onSessionView(
      sessionView("error", { state: "failed", reason: { kind: "auth-failed" } }, {
        mode: "desktop-sharing",
        explanation: { title: "Wrong user name or password", message: "m", next_steps: [], actions: ["edit-profile", "reconnect"] },
      }),
    );
    const edit = button(root, "Edit Connection…");
    expect(edit.classList.contains("primary")).toBe(true);
    edit.click();
    await flush();
    expect(fake.calls).toContainEqual(["showConnections", PROFILE_ID]);
    // The failed window stays until it is closed.
    expect(fake.names()).not.toContain("closeSession");
  });

  test("the page never shows a profiles form", async () => {
    const { root, app } = await start();
    expect(root.querySelector("form")).toBeNull();
    app.onSessionView(sessionView("live", LIVE));
    expect(root.querySelector("form")).toBeNull();
  });

  test("live desktop clears the page", async () => {
    const { root, app } = await start();
    app.onSessionView(sessionView("live", LIVE));
    expect(document.body.dataset.screen).toBe("live");
    expect(root.childElementCount).toBe(0);
  });

  test("the statistics HUD floats over the live picture when it is switched on", async () => {
    const { root, app } = await start();
    app.onSessionView(sessionView("live", LIVE, { stats: stats() }));
    expect(root.childElementCount).toBe(0);
    expect(document.body.dataset.hud ?? "").toBe("");
    app.onSessionView(sessionView("live", LIVE, { show_stats: true, stats: stats() }));
    expect(document.body.dataset.hud).toBe("stats");
    expect(text(root.querySelector(".hud.stats"))).toContain("58.9 fps");
    app.onSessionView(sessionView("live", LIVE, { show_stats: true, stats: null }));
    expect(root.childElementCount).toBe(0);
    expect(document.body.dataset.hud ?? "").toBe("");
  });
});
