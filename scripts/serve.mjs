import { createReadStream, existsSync, statSync } from "node:fs";
import { createServer } from "node:http";
import { extname, normalize, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
const preferredPort = Number(process.env.PORT ?? 4173);
const host = process.env.HOST ?? "127.0.0.1";

const contentTypes = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
  ".webp": "image/webp",
  ".svg": "image/svg+xml",
  ".wasm": "application/wasm",
};

function filePathFor(requestUrl) {
  const url = new URL(requestUrl ?? "/", `http://${host}`);
  const requested = decodeURIComponent(url.pathname) === "/" ? "index.html" : decodeURIComponent(url.pathname).replace(/^\/+/, "");
  return resolve(root, normalize(requested));
}

function listen(port) {
  return new Promise((resolveListen, reject) => {
    const server = createServer((request, response) => {
      let filePath;
      try {
        filePath = filePathFor(request.url);
      } catch {
        response.writeHead(400).end("Bad request");
        return;
      }

      if (!filePath.startsWith(`${root}/`) || !existsSync(filePath) || !statSync(filePath).isFile()) {
        response.writeHead(404, { "content-type": "text/plain; charset=utf-8" }).end("Not found");
        return;
      }

      response.writeHead(200, {
        "content-type": contentTypes[extname(filePath)] ?? "application/octet-stream",
        "cache-control": "no-store",
      });
      createReadStream(filePath).pipe(response);
    });

    server.once("error", reject);
    server.listen(port, host, () => resolveListen(server));
  });
}

let server;
for (let port = preferredPort; port < preferredPort + 20; port += 1) {
  try {
    server = await listen(port);
    break;
  } catch (error) {
    if (error?.code !== "EADDRINUSE") throw error;
  }
}

if (!server) throw new Error(`No free port found from ${preferredPort} to ${preferredPort + 19}`);

const address = server.address();
console.log(`NEURALYZED running at http://${host}:${address.port}/`);
