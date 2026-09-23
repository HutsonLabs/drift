// Saved connections sidebar: a search field, one row per connection (mode glyph, name, host,
// and a green dot when it is open in another tab) and "New Connection". Every row keeps its own
// Connect button; it is drawn only on the selected or hovered row but stays in the tab order.
// Connecting a connection that is open in another tab switches to that tab (Rust decides).
import type { ConnectMode, ProfileEntry_Serialize } from "../bindings";
import { h } from "../dom";
import { icon, modeGlyph } from "../icons";

export const MODE_NAMES: Record<ConnectMode, string> = {
  "remote-login": "Remote Login",
  headless: "Headless",
  "desktop-sharing": "Desktop Sharing",
};

/** What the list can ask for. */
export interface ListIntents {
  select(id: string): void;
  connect(id: string): void;
  create(): void;
  /** The search text changed (the list filters itself; this only remembers it). */
  search?(query: string): void;
}

/** Whether a connection matches the search text (name, host or mode, case-insensitive). */
export function matches(entry: ProfileEntry_Serialize, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (q === "") return true;
  const p = entry.profile;
  return [p.name, p.host, MODE_NAMES[p.mode]].some((s) => s.toLowerCase().includes(q));
}

/** The sidebar element. */
export function profileList(
  entries: ProfileEntry_Serialize[],
  selectedId: string | null,
  on: ListIntents,
  query = "",
  live: ReadonlySet<string> = new Set(),
): HTMLElement {
  const rows = entries.map((entry) => {
    const p = entry.profile;
    const li = h(
      "li",
      { class: p.id === selectedId ? "selected" : null, hidden: !matches(entry, query) },
      h(
        "button",
        {
          type: "button",
          class: "item",
          "aria-label": p.name,
          "aria-current": p.id === selectedId ? "true" : null,
          onclick: () => on.select(p.id),
        },
        modeGlyph(p.mode),
        h(
          "span",
          { class: "item-text" },
          h("span", { class: "item-name" }, p.name),
          h("span", { class: "item-detail" }, `${p.host} · ${MODE_NAMES[p.mode]}`),
        ),
        live.has(p.id)
          ? h("span", { class: "status live", title: "Connected in another tab", role: "img", "aria-label": "Connected in another tab" })
          : null,
      ),
      h(
        "button",
        { type: "button", class: "connect", "aria-label": `Connect to ${p.name}`, onclick: () => on.connect(p.id) },
        icon("play"),
      ),
    );
    return { entry, li };
  });

  const empty = h("p", { class: "no-results", hidden: rows.some((r) => !r.li.hidden) || entries.length === 0 }, "No matches");
  const search = h("input", {
    type: "search",
    class: "search-field",
    placeholder: "Search",
    "aria-label": "Search connections",
    value: query,
    oninput: () => {
      for (const r of rows) r.li.hidden = !matches(r.entry, search.value);
      empty.hidden = rows.some((r) => !r.li.hidden);
      on.search?.(search.value);
    },
  });

  return h(
    "nav",
    { class: "sidebar", "aria-label": "Saved connections" },
    h("div", { class: "search" }, icon("search"), search),
    h("h2", {}, "Connections"),
    h("ul", {}, rows.map((r) => r.li)),
    empty,
    h(
      "div",
      { class: "sidebar-foot" },
      h("button", { type: "button", class: "new", onclick: () => on.create() }, icon("plus"), "New Connection"),
    ),
  );
}
