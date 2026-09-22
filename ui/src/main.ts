// Webview entry point (humble): wires the generated tauri-specta bindings to the controller.
// Everything testable lives in app.ts and views/; this file only touches the Tauri runtime.
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { commands, events } from "./bindings";
import { DriftApp } from "./app";

const root = document.getElementById("app");
if (root) {
  const app = new DriftApp(root, commands);
  void app.start();
  // Session views are emitted to this window only (SessionManager → emit_to(window)).
  void events.sessionViewChanged(getCurrentWebviewWindow()).listen((e) => app.onSessionView(e.payload));
}
