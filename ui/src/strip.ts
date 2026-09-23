// Tab strip webview entry point (humble): wires the generated bindings to the strip controller.
// Everything testable lives in stripApp.ts and views/tabStrip.ts.
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { commands, events } from "./bindings";
import { StripApp } from "./stripApp";

const root = document.getElementById("strip");
if (root) {
  const app = new StripApp(root, commands);
  // Strips are emitted to this webview only (windows::broadcast_tabs → emit_to(webview)).
  void events.tabStripChanged(getCurrentWebview()).listen((e) => app.onStrip(e.payload));
  void app.start();
  // A click on the strip makes its web view first responder; the keyboard belongs to the page
  // or the remote desktop, never to the strip.
  window.addEventListener("focus", () => app.focused());
}
