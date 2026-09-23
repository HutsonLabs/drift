// The tab strip (UI-tabs, docs/design/mockup-glass.html boards 0–5): the transparent title bar
// row beside the traffic lights. Rust pushes the model (`TabStrip`); this only renders it.
//
// - Connection Manager tabs: neutral glyph, "Connections".
// - Session tabs: mode glyph, name, status dot (green live, amber reconnecting, red failed);
//   while connecting a spinner instead of the glyph.
// - The window's own tab is the raised glass pill with its ×; the others are flat and show the
//   × on hover.
// - A Firefox-style + follows the last tab and opens a new Connection Manager tab.
// - The empty rest of the row drags the window (Tauri's `data-tauri-drag-region`).
import type { TabItem, TabStatus, TabStrip } from "../bindings";
import { h, mount } from "../dom";
import { icon, modeGlyph } from "../icons";

/** What the strip can ask for. */
export interface StripIntents {
  /** Show another tab. */
  select(id: string): void;
  /** Close a tab (its session closes gracefully first). */
  close(id: string): void;
  /** Open a new Connection Manager tab. */
  newTab(): void;
  /** The window's own tab was clicked: hand the keyboard back to the page or the picture. */
  focus(): void;
}

/** What VoiceOver hears for a tab's spinner or dot. */
export const STATUS_TEXT: Record<TabStatus, string> = {
  idle: "",
  connecting: "Connecting",
  live: "Connected",
  reconnecting: "Reconnecting",
  failed: "Disconnected",
};

function glyph(tab: TabItem): HTMLElement {
  if (tab.status === "connecting") return h("span", { class: "spin", "aria-hidden": "true" });
  if (tab.kind === "manager" || !tab.mode) {
    return h("span", { class: "glyph glyph-manager", "aria-hidden": "true" }, icon("manager"));
  }
  return modeGlyph(tab.mode);
}

/** The DOM id of tab `id`'s role="tab" button (the tablist owns it by id). */
function tabElementId(id: string): string {
  return `tab-${id}`;
}

function renderTab(tab: TabItem, active: boolean, on: StripIntents): HTMLElement {
  const status = STATUS_TEXT[tab.status];
  const statusId = `tab-status-${tab.id}`;
  const tabId = tabElementId(tab.id);
  const dot = tab.kind === "session" && tab.status !== "connecting" && tab.status !== "idle";
  return h(
    "div",
    {
      class: active ? "tab on" : "tab",
      "data-id": tab.id,
      "data-status": tab.status,
      title: tab.hint ?? tab.title,
    },
    h(
      "button",
      {
        type: "button",
        id: tabId,
        class: "tab-main",
        role: "tab",
        "aria-selected": active ? "true" : "false",
        "aria-label": tab.title,
        "aria-describedby": status ? statusId : null,
        onclick: () => (active ? on.focus() : on.select(tab.id)),
      },
      glyph(tab),
      h("span", { class: "t-name" }, tab.title),
      dot ? h("span", { class: `status ${tab.status}`, "aria-hidden": "true" }) : null,
      status ? h("span", { id: statusId, class: "visually-hidden" }, status) : null,
    ),
    h(
      "button",
      {
        type: "button",
        class: "close",
        "aria-label": `Close ${tab.title}`,
        "aria-controls": tabId,
        onclick: () => on.close(tab.id),
      },
      icon("close"),
    ),
  );
}

/** Renders `strip` into `root`. */
export function renderTabStrip(root: HTMLElement, strip: TabStrip, on: StripIntents): void {
  const tabs = strip.tabs.map((tab) => renderTab(tab, tab.id === strip.active, on));
  const plus = h(
    "button",
    { type: "button", class: "newtab", "aria-label": "New Tab", title: "New Tab (⌘T)", onclick: () => on.newTab() },
    icon("plus"),
  );
  mount(
    root,
    h(
      "nav",
      { class: "tabstrip", "aria-label": "Tabs", "data-tauri-drag-region": true },
      // The + sits right after the last tab (Firefox-style); the rest of the row drags the window.
      // The row itself has no role: each pill holds a tab and its ×, and a tablist may own only
      // tabs. So the (boxless) tablist owns the tab buttons by id, and the × and + stay outside
      // it, so VoiceOver counts "tab 1 of N" right.
      h("div", {
        class: "tablist",
        role: "tablist",
        "aria-label": "Open tabs",
        "aria-owns": strip.tabs.map((tab) => tabElementId(tab.id)).join(" "),
      }),
      h("div", { class: "tabs" }, tabs, plus),
      h("span", { class: "spacer", "data-tauri-drag-region": true }),
    ),
  );
}
