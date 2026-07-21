import { mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";

type Options = { outFile: string; reportDir: string };

const options = parseArgs(Bun.argv.slice(2));

function parseArgs(args: string[]): Options {
  const result: Options = { outFile: "dist/neuralyzed.html", reportDir: "dist/single-html-report" };
  for (const arg of args) {
    if (arg.startsWith("--out=")) result.outFile = arg.slice(6);
    else if (arg.startsWith("--report-dir=")) result.reportDir = arg.slice(13);
    else if (arg === "--help" || arg === "-h") {
      console.log("Usage: bun src/isometric-rust-wasm-export-single-html.ts [--out=FILE] [--report-dir=DIR]");
      process.exit(0);
    } else throw new Error(`Unknown argument: ${arg}`);
  }
  return result;
}

function run(command: string, args: string[]) {
  const result = spawnSync(command, args, { cwd: process.cwd(), stdio: "inherit" });
  if (result.status !== 0) throw new Error(`${command} failed with ${result.status}`);
}

function scriptSafe(value: string) {
  return value.replace(/<\/script/gi, "<\\/script");
}

function embedHtmlImages(html: string) {
  return html.replace(/src="(\.\/assets\/[^"]+\.png)"/g, (_match, relative: string) => {
    const source = relative.slice(2);
    const safeName = source.replace(/[^a-z0-9]+/gi, "-").replace(/^-|-$/g, "").toLowerCase();
    const output = `dist/rust-wasm/export-${safeName}.webp`;
    run("cwebp", ["-quiet", "-lossless", "-z", "9", source, "-o", output]);
    return `src="data:image/webp;base64,${readFileSync(output).toString("base64")}"`;
  });
}

function embedFavicon(html: string) {
  return html.replace(/href="(\.\/assets\/branding\/[^\"]+\.png)"/g, (_match, relative: string) => {
    const data = readFileSync(relative.slice(2)).toString("base64");
    return `href="data:image/png;base64,${data}"`;
  });
}

function embedCssImages(html: string) {
  return html.replace(/url\(['"](\.\/assets\/ui\/hud-icons\/[^'"]+\.png)['"]\)/g, (_match, relative: string) => {
    const data = readFileSync(relative.slice(2)).toString("base64");
    return `url('data:image/png;base64,${data}')`;
  });
}

run("bun", ["run", "build:rust-wasm"]);
const atlasPngPath = "dist/rust-wasm/iso-atlas.png";
const atlasWebpPath = "dist/rust-wasm/iso-atlas.webp";
const atlasHashPath = `${atlasWebpPath}.sha256`;
const atlasHash = createHash("sha256").update(readFileSync(atlasPngPath)).digest("hex");
let cachedHash = "";
try { cachedHash = readFileSync(atlasHashPath, "utf8").trim(); } catch {}
if (cachedHash !== atlasHash) {
  run("cwebp", ["-quiet", "-lossless", "-z", "9", atlasPngPath, "-o", atlasWebpPath]);
  writeFileSync(atlasHashPath, atlasHash + "\n");
}

const [browserBuild, workerBuild] = await Promise.all([
  Bun.build({ entrypoints: ["src/isometric-rust-wasm-browser.ts"], target: "browser", format: "esm", minify: true }),
  Bun.build({ entrypoints: ["src/runtime/rust-wasm-worker.ts"], target: "browser", format: "esm", minify: true }),
]);
if (!browserBuild.success || !workerBuild.success) {
  for (const log of [...browserBuild.logs, ...workerBuild.logs]) console.error(log);
  process.exit(1);
}

const browserBundle = await browserBuild.outputs[0].text();
const workerBundle = (await workerBuild.outputs[0].text()).replaceAll("import.meta.url", "self.location.href");
const wasmBase64 = readFileSync("dist/rust-wasm/mib_rust_wasm_bg.wasm").toString("base64");
const atlasBase64 = readFileSync("dist/rust-wasm/iso-atlas.webp").toString("base64");
const atlasMeta = JSON.parse(readFileSync("dist/rust-wasm/iso-atlas-meta.json", "utf8"));
const workerSource = `self.__MIB_RUST_WASM_BASE64__=${JSON.stringify(wasmBase64)};\n${workerBundle}`;

let html = embedFavicon(embedCssImages(embedHtmlImages(readFileSync("index.html", "utf8"))))
  .replaceAll("url('./dist/rust-wasm/iso-atlas.png?v=20260721.1')", "var(--mib-embedded-atlas)")
  .replace(/\s*<script type="module" src="\.\/dist\/isometric-rust-wasm-browser\.js(?:\?[^\"]*)?"><\/script>/, "");

const bootstrap = [
  "<script>",
  `const atlasBinary=atob(${JSON.stringify(atlasBase64)}),atlasBytes=new Uint8Array(atlasBinary.length);for(let i=0;i<atlasBinary.length;i++)atlasBytes[i]=atlasBinary.charCodeAt(i);const atlasUrl=URL.createObjectURL(new Blob([atlasBytes],{type:"image/webp"}));document.documentElement.style.setProperty("--mib-embedded-atlas",` + "`url(\"${atlasUrl}\")`" + `);window.__MIB_RUST_EMBEDDED_ASSETS__={atlasUrl,atlasMeta:${JSON.stringify(atlasMeta)}};`,
  `window.__MIB_RUST_WORKER_URL__=URL.createObjectURL(new Blob([${JSON.stringify(workerSource)}],{type:"text/javascript"}));`,
  "</script>",
  `<script type="module">${scriptSafe(browserBundle)}</script>`,
].join("\n");
html = html.replace("</body>", `${bootstrap}\n</body>`);

mkdirSync(dirname(options.outFile), { recursive: true });
writeFileSync(options.outFile, html);
const report = {
  createdAt: new Date().toISOString(),
  output: options.outFile,
  design: "glass",
  htmlBytes: statSync(options.outFile).size,
  wasmBytes: statSync("dist/rust-wasm/mib_rust_wasm_bg.wasm").size,
  atlasPngBytes: statSync("dist/rust-wasm/iso-atlas.png").size,
  atlasWebpBytes: statSync("dist/rust-wasm/iso-atlas.webp").size,
  workerBundleBytes: Buffer.byteLength(workerBundle),
  browserBundleBytes: Buffer.byteLength(browserBundle),
};
mkdirSync(options.reportDir, { recursive: true });
writeFileSync(join(options.reportDir, "latest.json"), JSON.stringify(report, null, 2) + "\n");
writeFileSync(join(options.reportDir, "latest.md"), [
  "# Rust WASM Single HTML Export", "", `Created: ${report.createdAt}`, `Output: \`${report.output}\``,
  `HTML: ${(report.htmlBytes / 1024 / 1024).toFixed(2)} MiB`, `WASM: ${(report.wasmBytes / 1024).toFixed(1)} KiB`,
  `Atlas PNG source: ${(report.atlasPngBytes / 1024 / 1024).toFixed(2)} MiB`,
  `Atlas WebP embedded: ${(report.atlasWebpBytes / 1024 / 1024).toFixed(2)} MiB`,
].join("\n") + "\n");
console.log(JSON.stringify({ ...report, htmlMiB: Number((report.htmlBytes / 1024 / 1024).toFixed(2)) }, null, 2));
