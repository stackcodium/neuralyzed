import { spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const appUrl = process.env.APP_URL ?? "http://127.0.0.1:4173/";
const browser = process.env.BROWSER_BIN ?? [
  "/usr/bin/brave-browser",
  "/usr/bin/google-chrome",
  "/usr/bin/chromium",
].find(existsSync);

if (!browser) throw new Error("No supported browser found. Set BROWSER_BIN to a Chromium-compatible executable.");

const profile = mkdtempSync(join(tmpdir(), "neuralyzed-smoke-"));
const browserProcess = spawn(browser, [
  "--headless=new",
  "--enable-unsafe-swiftshader",
  "--no-first-run",
  "--no-default-browser-check",
  "--remote-debugging-port=0",
  `--user-data-dir=${profile}`,
  "about:blank",
], { stdio: ["ignore", "ignore", "pipe"] });

let browserErrors = "";
browserProcess.stderr.on("data", (chunk) => { browserErrors += chunk; });

const delay = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

async function devToolsTarget() {
  const portFile = join(profile, "DevToolsActivePort");
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (existsSync(portFile)) {
      const port = Number(readFileSync(portFile, "utf8").split("\n")[0]);
      const response = await fetch(`http://127.0.0.1:${port}/json/list`);
      const targets = await response.json();
      const page = targets.find((target) => target.type === "page");
      if (page) return page;
    }
    if (browserProcess.exitCode !== null) throw new Error(`Browser exited with ${browserProcess.exitCode}: ${browserErrors}`);
    await delay(100);
  }
  throw new Error("Timed out waiting for the browser debugging endpoint.");
}

let socket;
let closeBrowser;
try {
  const target = await devToolsTarget();
  socket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });

  let nextId = 1;
  const pending = new Map();
  const failures = [];
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      const { resolve, reject } = pending.get(message.id);
      pending.delete(message.id);
      if (message.error) reject(new Error(message.error.message));
      else resolve(message.result);
    }
    if (message.method === "Runtime.exceptionThrown") failures.push(message.params.exceptionDetails.text);
    if (message.method === "Log.entryAdded" && message.params.entry.level === "error") failures.push(message.params.entry.text);
    if (message.method === "Network.loadingFailed") failures.push(`${message.params.errorText}: ${message.params.blockedReason ?? "resource load"}`);
  });

  function send(method, params = {}) {
    const id = nextId++;
    socket.send(JSON.stringify({ id, method, params }));
    return new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
  }
  closeBrowser = () => send("Browser.close");

  async function evaluate(expression) {
    const result = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    if (result.exceptionDetails) throw new Error(result.exceptionDetails.text);
    return result.result.value;
  }

  await Promise.all([send("Page.enable"), send("Runtime.enable"), send("Log.enable"), send("Network.enable")]);
  await send("Page.navigate", { url: appUrl });

  let pageState;
  for (let attempt = 0; attempt < 100; attempt += 1) {
    pageState = await evaluate(`({ readyState: document.readyState, cards: document.querySelectorAll(".class-card").length, logos: [...document.querySelectorAll("#brand img, .classpick-logo")].filter((image) => image.complete && image.naturalWidth > 0).length })`);
    if (pageState.readyState === "complete" && pageState.cards === 5 && pageState.logos === 3) break;
    await delay(100);
  }
  if (pageState?.cards !== 5 || pageState.logos !== 3) throw new Error(`Agent picker did not load: ${JSON.stringify(pageState)}`);

  mkdirSync("reports", { recursive: true });
  const pickerScreenshot = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync("reports/browser-picker.png", Buffer.from(pickerScreenshot.data, "base64"));

  await evaluate(`document.querySelector('.class-card[data-class="a"]').click()`);
  let gameState;
  for (let attempt = 0; attempt < 200; attempt += 1) {
    gameState = await evaluate(`({ status: document.querySelector("#status")?.textContent ?? "", canvases: document.querySelectorAll("#canvasHost canvas").length, pickerVisible: document.querySelector("#classPicker")?.classList.contains("visible") })`);
    if (gameState.status.includes("Mission ready") && gameState.canvases >= 2 && !gameState.pickerVisible) break;
    await delay(100);
  }

  const screenshot = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync("reports/browser-smoke.png", Buffer.from(screenshot.data, "base64"));

  if (!gameState?.status.includes("Mission ready") || gameState.canvases < 2 || gameState.pickerVisible) {
    throw new Error(`Game did not become ready: ${JSON.stringify(gameState)}`);
  }
  if (failures.length) throw new Error(`Browser errors:\n${failures.join("\n")}`);

  console.log(JSON.stringify({ ok: true, url: appUrl, ...gameState, pickerScreenshot: "reports/browser-picker.png", screenshot: "reports/browser-smoke.png" }, null, 2));
} finally {
  await closeBrowser?.().catch(() => {});
  socket?.close();
  browserProcess.kill("SIGTERM");
  await Promise.race([new Promise((resolve) => browserProcess.once("exit", resolve)), delay(2000)]);
  if (browserProcess.exitCode === null) browserProcess.kill("SIGKILL");
  rmSync(profile, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
}
