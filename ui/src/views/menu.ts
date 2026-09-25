// A pop-up menu (the card's ⋯ and context menu): glass, one `menuitem` per action with its
// shortcut. Arrow keys move, Return / Space choose, Esc or a click outside closes it and the
// keyboard goes back to where it came from.
import { h, mount } from "../dom";

/** One menu entry. */
export interface MenuItem {
  label: string;
  /** Shown on the right, e.g. "⌘E". */
  shortcut?: string;
  /** For VoiceOver (`aria-keyshortcuts`), e.g. "Meta+E". */
  keys?: string;
  destructive?: boolean;
  run(): void;
}

/** Opens a menu named `label` in `layer` at `at` (viewport coordinates). */
export function openMenu(
  layer: HTMLElement,
  label: string,
  items: MenuItem[],
  at: { x: number; y: number },
  returnFocus: () => HTMLElement | null,
): void {
  const close = (refocus: boolean) => {
    mount(layer);
    if (refocus) returnFocus()?.focus();
  };
  const buttons = items.map((item) =>
    h(
      "button",
      {
        type: "button",
        role: "menuitem",
        tabindex: -1,
        class: item.destructive ? "destructive" : null,
        "aria-label": item.label,
        "aria-keyshortcuts": item.keys,
        onclick: () => {
          close(true);
          item.run();
        },
      },
      h("span", { class: "label" }, item.label),
      item.shortcut ? h("kbd", { "aria-hidden": "true" }, item.shortcut) : "",
    ),
  );
  const menu = h("div", { class: "menu", role: "menu", "aria-label": label }, buttons);
  menu.addEventListener("keydown", (e) => {
    const i = buttons.indexOf(document.activeElement as HTMLButtonElement);
    const n = buttons.length;
    const go = (j: number) => {
      e.preventDefault();
      buttons[(j + n) % n]?.focus();
    };
    if (e.key === "ArrowDown") go(i + 1);
    else if (e.key === "ArrowUp") go(i < 0 ? n - 1 : i - 1);
    else if (e.key === "Home") go(0);
    else if (e.key === "End") go(n - 1);
    else if (e.key === "Escape" || e.key === "Tab") {
      e.preventDefault();
      e.stopPropagation();
      close(true);
    }
  });
  const backdrop = h("div", { class: "menu-backdrop" });
  backdrop.addEventListener("mousedown", (e) => {
    e.preventDefault();
    close(true);
  });
  backdrop.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    close(true);
  });
  // Keep the menu on screen.
  const x = Math.max(8, Math.min(at.x, window.innerWidth - 232));
  const y = Math.max(8, Math.min(at.y, window.innerHeight - (items.length * 28 + 16)));
  menu.style.left = `${x}px`;
  menu.style.top = `${y}px`;
  mount(layer, backdrop, menu);
  buttons[0]?.focus();
}
