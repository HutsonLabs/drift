// UI-tabs Red: the HTML tab strip in the transparent title bar row
// (docs/design/mockup-glass.html boards 0–5). Rust pushes the model; the strip only renders it
// and turns clicks into intents.
import { describe, expect, test } from "bun:test";
import type { CommandError, TabItem, TabStrip } from "../src/bindings";
import { type StripApi, StripApp } from "../src/stripApp";
import { renderTabStrip } from "../src/views/tabStrip";
import { button, flush, hasButton, text } from "./helpers";

type Result<T> = { status: "ok"; data: T } | { status: "error"; error: CommandError };
const ok = <T>(data: T): Promise<Result<T>> => Promise.resolve({ status: "ok", data });

function manager(id: string): TabItem {
  return { id, kind: "manager", title: "Connections", mode: null, status: "idle", profile_id: null, hint: null };
}

function session(id: string, title: string, over: Partial<TabItem> = {}): TabItem {
  return { id, kind: "session", title, mode: "remote-login", status: "live", profile_id: null, hint: null, ...over };
}

function strip(tabs: TabItem[], active: string): TabStrip {
  return { tabs, active, live_profiles: [] };
}

class Intents {
  calls: string[][] = [];
  select = (id: string) => this.calls.push(["select", id]);
  close = (id: string) => this.calls.push(["close", id]);
  newTab = () => this.calls.push(["newTab"]);
  focus = () => this.calls.push(["focus"]);
}

function render(model: TabStrip) {
  const root = document.createElement("div");
  document.body.replaceChildren(root);
  const on = new Intents();
  renderTabStrip(root, model, on);
  return { root, on };
}

const tabs = (root: ParentNode) => Array.from(root.querySelectorAll<HTMLElement>(".tab"));

describe("tab strip", () => {
  test("a new window shows one Connection Manager tab with + right after it", () => {
    const { root } = render(strip([manager("session-0")], "session-0"));
    const list = root.querySelector("[role=tablist]");
    expect(list).not.toBeNull();
    const [tab] = tabs(root);
    expect(tab?.classList.contains("on")).toBe(true);
    expect(tab?.querySelector(".glyph-manager")).not.toBeNull();
    expect(text(tab)).toContain("Connections");
    const main = button(root, "Connections");
    expect(main.getAttribute("role")).toBe("tab");
    expect(main.getAttribute("aria-selected")).toBe("true");
    expect(hasButton(root, "Close Connections")).toBe(true);
    // Firefox-style: the + follows the last tab instead of sitting at the far edge.
    expect(tab?.nextElementSibling).toBe(button(root, "New Tab"));
  });

  test("a connecting tab shows a spinner and “Connecting to <name>…”", () => {
    const { root } = render(strip([session("session-0", "Connecting to Homelab…", { status: "connecting" })], "session-0"));
    const [tab] = tabs(root);
    expect(tab?.querySelector(".spin")).not.toBeNull();
    expect(tab?.querySelector(".glyph")).toBeNull();
    expect(text(tab)).toContain("Connecting to Homelab…");
    expect(tab?.querySelector(".status")).toBeNull();
  });

  test("session tabs show the mode glyph, the name and a status dot", () => {
    const { root } = render(
      strip(
        [
          session("session-0", "Homelab"),
          session("session-1", "Studio Workstation", { mode: "headless", status: "reconnecting" }),
          session("session-2", "Kitchen", { mode: "desktop-sharing", status: "failed" }),
        ],
        "session-0",
      ),
    );
    const [a, b, c] = tabs(root);
    expect(a?.querySelector(".glyph-remote-login")).not.toBeNull();
    expect(a?.querySelector(".status.live")).not.toBeNull();
    expect(b?.querySelector(".glyph-headless")).not.toBeNull();
    expect(b?.querySelector(".status.reconnecting")).not.toBeNull();
    expect(c?.querySelector(".glyph-desktop-sharing")).not.toBeNull();
    expect(c?.querySelector(".status.failed")).not.toBeNull();
  });

  test("only the window's own tab is the raised pill", () => {
    const { root } = render(strip([session("session-0", "Homelab"), manager("session-1")], "session-1"));
    const [a, b] = tabs(root);
    expect(a?.classList.contains("on")).toBe(false);
    expect(b?.classList.contains("on")).toBe(true);
    expect(button(root, "Homelab").getAttribute("aria-selected")).toBe("false");
    expect(button(root, "Connections").getAttribute("aria-selected")).toBe("true");
  });

  test("tabs keep the order Rust sends", () => {
    const { root } = render(strip([manager("session-3"), session("session-0", "Homelab"), manager("session-5")], "session-0"));
    expect(tabs(root).map((t) => t.dataset.id)).toEqual(["session-3", "session-0", "session-5"]);
  });

  test("clicks become intents: select another tab, close, new tab", () => {
    const { root, on } = render(strip([session("session-0", "Homelab"), manager("session-1")], "session-1"));
    button(root, "Homelab").click();
    button(root, "Close Homelab").click();
    button(root, "New Tab").click();
    button(root, "Connections").click();
    expect(on.calls).toEqual([["select", "session-0"], ["close", "session-0"], ["newTab"], ["focus"]]);
  });

  test("the greeter hint is the session tab's tooltip", () => {
    const hint = "Log in as “hutson” to start your session";
    const { root } = render(strip([session("session-0", "Homelab", { hint })], "session-0"));
    expect(tabs(root)[0]?.getAttribute("title")).toBe(hint);
  });

  test("the strip background drags the window; the tabs do not", () => {
    const { root } = render(strip([manager("session-0")], "session-0"));
    const nav = root.querySelector("nav");
    expect(nav?.hasAttribute("data-tauri-drag-region")).toBe(true);
    expect(nav?.getAttribute("aria-label")).toBe("Tabs");
    expect(tabs(root)[0]?.hasAttribute("data-tauri-drag-region")).toBe(false);
  });
});

class FakeStripApi {
  calls: unknown[][] = [];
  model: TabStrip = strip([manager("session-0")], "session-0");

  api(): StripApi {
    const log = (...c: unknown[]) => this.calls.push(c);
    return {
      tabStrip: () => (log("tabStrip"), ok(this.model)),
      selectTab: (tab: string) => (log("selectTab", tab), ok(null)),
      closeTab: (tab: string) => (log("closeTab", tab), ok(null)),
      newTab: () => (log("newTab"), ok(null)),
      focusContent: () => (log("focusContent"), ok(null)),
    } as StripApi;
  }
}

describe("strip controller", () => {
  test("loads the current strip on start and re-renders on every push", async () => {
    const fake = new FakeStripApi();
    const root = document.createElement("div");
    document.body.replaceChildren(root);
    const app = new StripApp(root, fake.api());
    await app.start();
    expect(fake.calls).toContainEqual(["tabStrip"]);
    expect(text(root)).toContain("Connections");

    app.onStrip(strip([session("session-0", "Homelab"), manager("session-1")], "session-0"));
    expect(tabs(root)).toHaveLength(2);
    expect(text(root)).toContain("Homelab");
  });

  test("clicks go to Rust as commands", async () => {
    const fake = new FakeStripApi();
    fake.model = strip([session("session-0", "Homelab"), manager("session-1")], "session-1");
    const root = document.createElement("div");
    document.body.replaceChildren(root);
    const app = new StripApp(root, fake.api());
    await app.start();
    button(root, "Homelab").click();
    button(root, "Close Homelab").click();
    button(root, "New Tab").click();
    button(root, "Connections").click();
    await flush();
    expect(fake.calls.slice(1)).toEqual([["selectTab", "session-0"], ["closeTab", "session-0"], ["newTab"], ["focusContent"]]);
  });
});
