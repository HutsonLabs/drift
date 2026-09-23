// Controller of a window's tab strip webview (UI-tabs): loads the current strip, re-renders on
// every push from Rust, and sends clicks to Rust as commands. The API is injected (the
// generated tauri-specta `commands` in production, a fake in tests).
import type { CommandError, TabStrip, commands } from "./bindings";
import { renderTabStrip } from "./views/tabStrip";

/** The IPC commands the strip uses. */
export type StripApi = Pick<typeof commands, "tabStrip" | "selectTab" | "closeTab" | "newTab" | "focusContent">;

type Result<T> = { status: "ok"; data: T } | { status: "error"; error: CommandError };

/** The strip webview controller. */
export class StripApp {
  private strip: TabStrip | null = null;

  constructor(
    private readonly root: HTMLElement,
    private readonly api: StripApi,
  ) {}

  /** Loads the strip (a push may already have arrived; then this is a no-op re-render). */
  async start(): Promise<void> {
    const r = (await this.api.tabStrip()) as Result<TabStrip>;
    if (r.status === "ok" && this.strip === null) this.onStrip(r.data);
  }

  /** A new strip from Rust (the `tabStripChanged` event). */
  onStrip(strip: TabStrip): void {
    this.strip = strip;
    renderTabStrip(this.root, strip, {
      select: (id) => void this.api.selectTab(id),
      close: (id) => void this.api.closeTab(id),
      newTab: () => void this.api.newTab(),
      focus: () => void this.api.focusContent(),
    });
  }

  /** The strip webview became first responder: give the keyboard back to the page or picture. */
  focused(): void {
    void this.api.focusContent();
  }
}
