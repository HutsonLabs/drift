import { describe, expect, test } from "bun:test";
import { renderHome } from "../src/app";

describe("renderHome", () => {
  test("shows a loading placeholder until the version is known", () => {
    const root = document.createElement("main");
    renderHome(root, { name: "Drift", version: null });
    expect(root.querySelector("h1")?.textContent).toBe("Drift");
    expect(root.querySelector(".version")?.textContent).toBe("Loading…");
  });

  test("replaces previous content with the version", () => {
    const root = document.createElement("main");
    renderHome(root, { name: "Drift", version: null });
    renderHome(root, { name: "Drift", version: "0.1.0" });
    expect(root.querySelectorAll("section").length).toBe(1);
    expect(root.querySelector(".version")?.textContent).toBe("Version 0.1.0");
  });
});
