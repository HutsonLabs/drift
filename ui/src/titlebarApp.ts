// Controller of a session window's title-bar webview (UI-windows decisions 5, 6 and 11): pulls
// the window's identity, re-renders on every `windowIdentityChanged` push, sends the buttons'
// clicks to Rust, and gives the keyboard straight back whenever the title bar gains focus.
import type { Result } from "./api";
import type { WindowIdentity, commands } from "./bindings";
import { renderIdentity } from "./views/identity";

/** The IPC commands the title bar uses. */
export type TitlebarApi = Pick<typeof commands, "windowIdentity" | "showConnections" | "toggleStats" | "focusContent">;

/** The title-bar webview controller. */
export class TitlebarApp {
  private identity: WindowIdentity | null = null;
  private readonly onFocus = () => void this.api.focusContent();

  constructor(
    private readonly root: HTMLElement,
    private readonly api: TitlebarApi,
  ) {}

  /** Loads the identity (a push may have arrived first) and starts handing focus back. */
  async start(): Promise<void> {
    window.addEventListener("focus", this.onFocus);
    const r = (await this.api.windowIdentity()) as Result<WindowIdentity>;
    if (r.status === "ok" && this.identity === null) this.onIdentity(r.data);
  }

  /** A new identity from Rust (the `windowIdentityChanged` event). */
  onIdentity(identity: WindowIdentity): void {
    this.identity = identity;
    renderIdentity(this.root, identity, {
      showConnections: () => void this.api.showConnections(null),
      toggleStats: () => void this.api.toggleStats(),
    });
  }

  /** Stops listening for focus. */
  dispose(): void {
    window.removeEventListener("focus", this.onFocus);
  }
}
