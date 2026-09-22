// Pure view functions: state in, DOM out. No Tauri imports here so they are testable
// under happy-dom.

/** Static app information shown on the home screen. */
export interface HomeState {
  name: string;
  version: string | null;
}

/** Renders the home screen into `root`, replacing its contents. */
export function renderHome(root: HTMLElement, state: HomeState): void {
  const section = document.createElement("section");
  section.className = "drift-home";
  const title = document.createElement("h1");
  title.textContent = state.name;
  const version = document.createElement("p");
  version.className = "version";
  version.textContent = state.version === null ? "Loading…" : `Version ${state.version}`;
  section.append(title, version);
  root.replaceChildren(section);
}
