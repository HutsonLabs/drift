// UI-windows Red: three pages (Connections, session window, session title bar), no tab strip,
// and one mode order everywhere.
import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { DEFAULT_MODE, MODE_NAMES, MODE_ORDER } from "../src/modes";

const ui = (path: string) => new URL(`../${path}`, import.meta.url).pathname;

describe("pages", () => {
  test("each window kind has its page and entry script; the strip is gone", () => {
    const pages: [string, string, string][] = [
      ["index.html", "main.js", "connections"],
      ["session.html", "sessionMain.js", "session"],
      ["titlebar.html", "titlebarMain.js", "titlebar"],
    ];
    for (const [page, script, body] of pages) {
      const html = readFileSync(ui(page), "utf8");
      expect(html).toContain(`src="./${script}"`);
      expect(html).toContain(`<body class="${body}">`);
      expect(html).toContain('href="./styles.css"');
    }
    expect(existsSync(ui("strip.html"))).toBe(false);
    const build = readFileSync(ui("build.ts"), "utf8");
    for (const entry of ["./src/main.ts", "./src/sessionMain.ts", "./src/titlebarMain.ts"]) expect(build).toContain(entry);
    for (const page of ["index.html", "session.html", "titlebar.html"]) expect(build).toContain(page);
    expect(build).not.toContain("strip");
  });
});

describe("modes", () => {
  test("Headless → Desktop Sharing → Remote Login; new connections start Headless", () => {
    expect(MODE_ORDER).toEqual(["headless", "desktop-sharing", "remote-login"]);
    expect(DEFAULT_MODE).toBe("headless");
    expect(MODE_ORDER.map((m) => MODE_NAMES[m])).toEqual(["Headless session", "Desktop Sharing", "Remote Login"]);
  });
});
