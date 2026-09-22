// Tiny DOM builder (no framework): h("button", { class: "primary", onclick }, "Save").

type Child = Node | string | number | null | undefined | false;
type Handler = (event: Event) => void;
type AttrValue = string | number | boolean | null | undefined | Handler;

/** Creates an element. `on*` function props become listeners; `true` booleans become empty
 * attributes; `false`/`null`/`undefined` are skipped. Children may be nested arrays. */
export function h<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: Record<string, AttrValue> = {},
  ...children: (Child | Child[])[]
): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  for (const [key, value] of Object.entries(props)) {
    if (value === false || value === null || value === undefined) continue;
    if (typeof value === "function") {
      el.addEventListener(key.slice(2).toLowerCase(), value);
    } else if (key === "value" && (el instanceof HTMLInputElement || el instanceof HTMLSelectElement)) {
      el.value = String(value);
    } else if (key === "checked" && el instanceof HTMLInputElement) {
      el.checked = Boolean(value);
    } else {
      el.setAttribute(key, value === true ? "" : String(value));
    }
  }
  for (const child of children.flat()) {
    if (child === null || child === undefined || child === false) continue;
    el.append(child instanceof Node ? child : String(child));
  }
  return el;
}

/** Replaces the contents of `root` with `nodes`. */
export function mount(root: HTMLElement, ...nodes: Node[]): void {
  root.replaceChildren(...nodes);
}

/** A button. The first action on a screen gets `primary`. */
export function actionButton(label: string, onClick: () => void, primary = false, ariaLabel?: string): HTMLButtonElement {
  return h(
    "button",
    { type: "button", class: primary ? "primary" : null, "aria-label": ariaLabel, onclick: () => onClick() },
    label,
  );
}
