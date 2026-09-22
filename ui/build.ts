// Builds the webview bundle into ui/dist (consumed by tauri.conf.json frontendDist).
import { cp, mkdir, rm } from "node:fs/promises";

const outdir = new URL("./dist/", import.meta.url).pathname;
await rm(outdir, { recursive: true, force: true });
await mkdir(outdir, { recursive: true });

const result = await Bun.build({
  entrypoints: ["./src/main.ts"],
  outdir,
  target: "browser",
  minify: true,
  sourcemap: "linked",
});
if (!result.success) {
  for (const log of result.logs) console.error(log);
  process.exit(1);
}
await cp(new URL("./index.html", import.meta.url).pathname, `${outdir}index.html`);
await cp(new URL("./src/styles.css", import.meta.url).pathname, `${outdir}styles.css`);
console.log(`ui: built ${result.outputs.length} file(s) into dist/`);
