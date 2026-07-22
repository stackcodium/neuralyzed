# Development

## One-time setup

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.126 --locked
```

The CLI and Rust crate versions must match. The build checks this early and prints a direct error if the wrong CLI is active. Bun provides the TypeScript runner, test runner, server, and bundler; this project has no npm dependency installation step.

## Development loop

```sh
bun run dev
```

This builds the Rust core for `wasm32-unknown-unknown`, runs `wasm-bindgen`, bundles the browser and worker modules, and serves the repository with cache disabled. Source edits require restarting the command; `bun run serve` serves an existing build without rebuilding it.

The checked-in release build can also be served without Bun:

```sh
node scripts/serve.mjs
```

Run all checks before opening a pull request:

```sh
bun run test
bun run build
```

Rust formatting can be checked separately with:

```sh
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
```

## Architecture

```text
Browser UI ──> Web Worker ──> WASM bridge ──> Rust game + bot
     │                                             │
     └──── isometric renderer <── render snapshot ─┘
```

The Rust core is authoritative for rules, simulation, combat, inventory, RNG, and planning. The TypeScript client presents snapshots and owns browser-only behavior such as dialogs, keyboard input, effects, pacing, and rendering. Keep rule changes in Rust and presentation changes in TypeScript.

The worker keeps heavy planning off the UI thread. Strategic full search is the default; optional Adaptive planning uses a capped CPU probe and falls back to Quick search if the probe reaches its sub-second budget.

## Build outputs

- `bun run build` writes the normal browser build to `dist/`.
- `bun run export:single` writes `dist/neuralyzed.html`.
- The minimal generated runtime required by GitHub Pages is committed; other generated output and Cargo target directories are ignored by Git.

The single-file export embeds the WASM module, worker, atlas, portraits, and interface icons. It requires the `cwebp` command because the large atlas is losslessly compressed before embedding.

## Determinism

Fixtures in `rust/fixtures/` guard reproducible RNG and stored outcomes. Avoid changing random-number consumption incidentally. When rules or planning intentionally change, run the full test command and document why fixture updates are correct.

## Troubleshooting

### Rust cannot find the WASM target

Run `rustup target add wasm32-unknown-unknown`.

### The build reports the wrong wasm-bindgen version

Install the pinned CLI with `cargo install wasm-bindgen-cli --version 0.2.126 --locked --force`.

### The page is blank when opened from disk

Use `bun run dev`; browsers restrict workers and WASM on `file://` pages.

### Single-file export cannot find cwebp

Install the WebP command-line tools supplied by your operating system, then rerun `bun run export:single`. The normal multi-file build does not require `cwebp`.
