// Sidebar search: filters by name, host or mode without re-rendering.
import { describe, expect, test } from "bun:test";
import { matches, profileList } from "../src/views/profileList";
import { entry, profile, type } from "./helpers";

describe("connection search", () => {
  const lab = entry(profile("headless", { name: "Build Server", host: "build-01.lan" }));
  const home = entry(profile("remote-login", { name: "Homelab", host: "gnome.local" }));

  test("matches name, host and mode, ignoring case", () => {
    expect(matches(lab, "build")).toBe(true);
    expect(matches(home, "GNOME.")).toBe(true);
    expect(matches(home, "remote login")).toBe(true);
    expect(matches(lab, "gnome")).toBe(false);
    expect(matches(lab, "  ")).toBe(true);
  });

  test("typing hides rows that do not match and reports the query", () => {
    let query = "";
    const nav = profileList([lab, home], null, { select: () => {}, connect: () => {}, create: () => {}, search: (q) => (query = q) });
    document.body.replaceChildren(nav);
    type(nav.querySelector("input[type=search]") as HTMLInputElement, "home");
    const visible = Array.from(nav.querySelectorAll("li")).filter((li) => !li.hidden);
    expect(visible.length).toBe(1);
    expect(query).toBe("home");
    expect((nav.querySelector(".no-results") as HTMLElement).hidden).toBe(true);
  });
});
