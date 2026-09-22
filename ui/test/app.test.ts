// M1-6 / M3-2 / M7-3 Red: the controller renders screens from state and turns clicks into
// IPC intents (through an injected API with the generated bindings' shapes).
import { describe, expect, test } from "bun:test";
import type {
  CommandError,
  ConnectionProfile_Deserialize,
  ConnectMode,
  ProfileEntry_Serialize,
  ProfileIssue,
  SecretsUpdate,
} from "../src/bindings";
import { type Api, DriftApp } from "../src/app";
import {
  button,
  certPrompt,
  check,
  entry,
  field,
  flush,
  GRDCTL_FINGERPRINT,
  hasButton,
  labels,
  LNP_EXPLANATION,
  profile,
  sessionView,
  stats,
  text,
  type,
} from "./helpers";

type Result<T> = { status: "ok"; data: T } | { status: "error"; error: CommandError };
const ok = <T>(data: T): Promise<Result<T>> => Promise.resolve({ status: "ok", data });

class FakeApi {
  calls: unknown[][] = [];
  entries: ProfileEntry_Serialize[] = [];
  saveIssues: ProfileIssue[] | null = null;
  connectError: CommandError | null = null;

  api(): Api {
    const log = (...c: unknown[]) => this.calls.push(c);
    return {
      listProfiles: () => (log("listProfiles"), ok(this.entries)),
      newProfile: (mode: ConnectMode) => (log("newProfile", mode), Promise.resolve(profile(mode, { name: "", host: "", rdp_username: "", linux_username: null }))),
      validateProfile: (p: ConnectionProfile_Deserialize) => (log("validateProfile", p), Promise.resolve([])),
      saveProfile: (p: ConnectionProfile_Deserialize, s: SecretsUpdate) => {
        log("saveProfile", p, s);
        if (this.saveIssues) return Promise.resolve({ status: "error", error: { kind: "invalid", issues: this.saveIssues } });
        const e = entry(p as never, true, false);
        this.entries = [...this.entries.filter((x) => x.profile.id !== p.id), e];
        return ok(e);
      },
      deleteProfile: (id: string) => {
        log("deleteProfile", id);
        this.entries = this.entries.filter((x) => x.profile.id !== id);
        return ok(null);
      },
      forgetCertificate: (id: string) => (log("forgetCertificate", id), ok(this.entries.find((e) => e.profile.id === id)!)),
      openLocalNetworkSettings: () => (log("openLocalNetworkSettings"), ok(null)),
      connect: (id: string) => {
        log("connect", id);
        return this.connectError ? Promise.resolve({ status: "error", error: this.connectError }) : ok(null);
      },
      acceptCertificate: (fp: string, pin: boolean) => (log("acceptCertificate", fp, pin), ok(null)),
      rejectCertificate: () => (log("rejectCertificate"), ok(null)),
      reconnectNow: () => (log("reconnectNow"), ok(null)),
      cancelReconnect: () => (log("cancelReconnect"), ok(null)),
      closeSession: () => (log("closeSession"), ok(null)),
      disconnect: () => (log("disconnect"), ok(null)),
    } as Api;
  }

  names(): string[] {
    return this.calls.map((c) => String(c[0]));
  }
}

async function start(fake = new FakeApi()) {
  const root = document.createElement("main");
  document.body.replaceChildren(root);
  const app = new DriftApp(root, fake.api());
  await app.start();
  await flush();
  return { root, app, fake };
}

describe("profiles screen", () => {
  test("first launch shows an empty state with a new Remote Login form", async () => {
    const { root, fake } = await start();
    expect(fake.names()).toContain("listProfiles");
    expect(text(root)).toContain("Add a GNOME computer");
    expect(labels(root)).toContain("System RDP user");
    expect(document.body.dataset.screen).toBe("profiles");
  });

  test("saved profiles are listed with connect buttons", async () => {
    const fake = new FakeApi();
    fake.entries = [entry(profile("headless", { name: "Headless box" })), entry(profile("remote-login", { name: "Login" }))];
    const { root } = await start(fake);
    const nav = root.querySelector("nav[aria-label='Saved connections']");
    expect(text(nav)).toContain("Headless box");
    expect(text(nav)).toContain("Login");
    expect(hasButton(root, "Connect to Headless box")).toBe(true);
  });

  test("the mode switch changes the visible credential fields (M3-2)", async () => {
    const { root } = await start();
    expect(labels(root)).toContain("Linux user (optional)");
    check(field(root, "Desktop Sharing") as HTMLInputElement);
    await flush();
    expect(labels(root)).not.toContain("Linux user (optional)");
    expect(labels(root)).toContain("RDP user");
    check(field(root, "Remote Login") as HTMLInputElement);
    await flush();
    expect(labels(root)).toContain("System RDP user");
  });

  test("saving shows Rust's validation issues next to the fields", async () => {
    const fake = new FakeApi();
    fake.saveIssues = [{ field: "host", problem: "empty", message: "Host is required." }];
    const { root } = await start(fake);
    button(root, "Save").click();
    await flush();
    expect(field(root, "Host").getAttribute("aria-invalid")).toBe("true");
    expect(text(root)).toContain("Host is required.");
  });

  test("saving a valid profile stores it with its secrets and lists it", async () => {
    const fake = new FakeApi();
    const { root } = await start(fake);
    type(field(root, "Name"), "Lab");
    type(field(root, "Host"), "gnome.local");
    type(field(root, "System RDP user"), "sys");
    type(field(root, "System RDP password"), "pw-Fake1");
    button(root, "Save").click();
    await flush();
    const save = fake.calls.find((c) => c[0] === "saveProfile")!;
    expect((save[1] as ConnectionProfile_Deserialize).name).toBe("Lab");
    expect(save[2]).toEqual({ rdp_password: "pw-Fake1", linux_password: { action: "keep" } });
    expect(text(root.querySelector("nav"))).toContain("Lab");
  });

  test("connect sends the profile id; a backend error is shown", async () => {
    const fake = new FakeApi();
    const p = profile("headless", { name: "Box" });
    fake.entries = [entry(p)];
    fake.connectError = { kind: "no-session" };
    const { root } = await start(fake);
    button(root, "Connect to Box").click();
    await flush();
    expect(fake.calls).toContainEqual(["connect", p.id]);
    expect(text(root.querySelector("[role=alert]"))).toContain("This tab is not connected.");
  });

  test("delete removes the profile", async () => {
    const fake = new FakeApi();
    const p = profile("headless", { name: "Box" });
    fake.entries = [entry(p)];
    const { root } = await start(fake);
    button(root, "Box").click();
    await flush();
    button(root, "Delete").click();
    await flush();
    expect(fake.calls).toContainEqual(["deleteProfile", p.id]);
    expect(text(root)).toContain("Add a GNOME computer");
  });
});

describe("session screens", () => {
  test("certificate prompt → acceptCertificate(fingerprint, remember)", async () => {
    const { root, app, fake } = await start();
    app.onSessionView(sessionView("certificate", { state: "connecting", leg: 1, stage: "tls" }, { certificate: certPrompt() }));
    expect(document.body.dataset.screen).toBe("certificate");
    expect(text(root.querySelector(".fingerprint"))).toBe(GRDCTL_FINGERPRINT);
    button(root, "Connect").click();
    await flush();
    expect(fake.calls).toContainEqual(["acceptCertificate", GRDCTL_FINGERPRINT, true]);
  });

  test("certificate cancel → rejectCertificate", async () => {
    const { root, app, fake } = await start();
    app.onSessionView(sessionView("certificate", { state: "connecting", leg: 1, stage: "tls" }, { certificate: certPrompt() }));
    button(root, "Cancel").click();
    await flush();
    expect(fake.names()).toContain("rejectCertificate");
  });

  test("cancelling while connecting disconnects but keeps the tab", async () => {
    const fake = new FakeApi();
    const { app, root } = await start(fake);
    app.onSessionView(sessionView("connecting", { state: "connecting", leg: 1, stage: "tls" }));
    button(root, "Cancel").click();
    await flush();
    expect(fake.names()).toContain("disconnect");
    expect(fake.names()).not.toContain("closeSession");
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
    expect(text(root)).toContain("Session is still running — log in to resume");
  });

  test("Local Network error opens System Settings", async () => {
    const { root, app, fake } = await start();
    app.onSessionView(sessionView("error", { state: "failed", reason: { kind: "local-network-denied" } }, { explanation: LNP_EXPLANATION }));
    button(root, "Open Local Network Settings").click();
    await flush();
    expect(fake.names()).toContain("openLocalNetworkSettings");
  });

  test("error Reconnect and Close map to intents", async () => {
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

  test("Edit Connection goes back to the form of the profile being connected", async () => {
    const fake = new FakeApi();
    const p = profile("desktop-sharing", { name: "Share" });
    fake.entries = [entry(p)];
    const { root, app } = await start(fake);
    button(root, "Connect to Share").click();
    await flush();
    app.onSessionView(
      sessionView("error", { state: "failed", reason: { kind: "auth-failed" } }, {
        mode: "desktop-sharing",
        explanation: { title: "Wrong user name or password", message: "m", next_steps: [], actions: ["edit-profile", "reconnect"] },
      }),
    );
    button(root, "Edit Connection…").click();
    await flush();
    expect(document.body.dataset.screen).toBe("profiles");
    expect((field(root, "Name") as HTMLInputElement).value).toBe("Share");
  });

  test("live desktop clears the webview", async () => {
    const { root, app } = await start();
    app.onSessionView(sessionView("live", { state: "connected", desktop: { width: 1280, height: 800 }, scale: 100 }));
    expect(document.body.dataset.screen).toBe("live");
    expect(root.childElementCount).toBe(0);
  });

  test("the statistics HUD floats over the live picture when it is switched on", async () => {
    const { root, app } = await start();
    const live = { state: "connected", desktop: { width: 1280, height: 800 }, scale: 100 } as const;
    app.onSessionView(sessionView("live", live, { stats: stats() }));
    expect(root.childElementCount).toBe(0);
    expect(document.body.dataset.hud ?? "").toBe("");

    app.onSessionView(sessionView("live", live, { show_stats: true, stats: stats() }));
    expect(document.body.dataset.screen).toBe("live");
    expect(document.body.dataset.hud).toBe("stats");
    expect(text(root.querySelector(".hud.stats"))).toContain("58.9 fps");

    // No sample yet: nothing to draw, and nothing covering the picture.
    app.onSessionView(sessionView("live", live, { show_stats: true, stats: null }));
    expect(root.childElementCount).toBe(0);
    expect(document.body.dataset.hud ?? "").toBe("");
  });

  test("the greeter hint is a banner over the picture", async () => {
    const { root, app } = await start();
    app.onSessionView(sessionView("greeter-hint", { state: "awaiting-greeter-login" }, { resuming: true }));
    expect(document.body.dataset.hud).toBe("banner");
    expect(text(root.querySelector(".banner"))).toContain("Session is still running");
  });

  test("back to profiles when the session is idle", async () => {
    const { root, app } = await start();
    app.onSessionView(sessionView("live", { state: "connected", desktop: { width: 1280, height: 800 }, scale: 100 }));
    app.onSessionView(sessionView("profiles", { state: "idle" }));
    expect(document.body.dataset.screen).toBe("profiles");
    expect(labels(root)).toContain("Name");
  });
});
