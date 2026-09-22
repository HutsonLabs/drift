// Webview entry point: wires Tauri IPC (generated bindings) to the pure view functions.
import { commands } from "./bindings";
import { renderHome } from "./app";

const root = document.getElementById("app");
if (root) {
  renderHome(root, { name: "Drift", version: null });
  commands
    .appInfo()
    .then((info) => renderHome(root, { name: info.name, version: info.version }))
    .catch(() => renderHome(root, { name: "Drift", version: "unknown" }));
}
