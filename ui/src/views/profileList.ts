// Saved connections sidebar.
import type { ConnectMode, ProfileEntry_Serialize } from "../bindings";
import { h } from "../dom";

const MODE_NAMES: Record<ConnectMode, string> = {
  "remote-login": "Remote Login",
  headless: "Headless",
  "desktop-sharing": "Desktop Sharing",
};

/** What the list can ask for. */
export interface ListIntents {
  select(id: string): void;
  connect(id: string): void;
  create(): void;
}

/** The sidebar element. */
export function profileList(entries: ProfileEntry_Serialize[], selectedId: string | null, on: ListIntents): HTMLElement {
  return h(
    "nav",
    { class: "sidebar", "aria-label": "Saved connections" },
    h("h2", {}, "Connections"),
    h(
      "ul",
      {},
      entries.map(({ profile: p }) =>
        h(
          "li",
          { class: p.id === selectedId ? "selected" : null },
          h(
            "button",
            {
              type: "button",
              class: "item",
              "aria-label": p.name,
              "aria-current": p.id === selectedId ? "true" : null,
              onclick: () => on.select(p.id),
            },
            h("span", { class: "item-name" }, p.name),
            h("span", { class: "item-detail" }, `${p.host} · ${MODE_NAMES[p.mode]}`),
          ),
          h(
            "button",
            { type: "button", class: "connect", "aria-label": `Connect to ${p.name}`, onclick: () => on.connect(p.id) },
            "Connect",
          ),
        ),
      ),
    ),
    h("button", { type: "button", class: "new", onclick: () => on.create() }, "New Connection"),
  );
}
