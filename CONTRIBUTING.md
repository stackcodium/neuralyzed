# Contributing

Thanks for helping improve NEURALYZED.

## Before you start

1. Run the game with `bun run dev` and reproduce the behavior you want to change.
2. Keep gameplay and bot decisions in the Rust core; keep rendering and browser interaction in TypeScript.
3. Prefer focused changes with tests that describe the intended behavior.

## Before submitting

```sh
bun run test
bun run build
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
```

Include a short explanation of the user-visible result, the tests performed, and screenshots for visual changes. Do not commit `dist/`, Cargo `target/` directories, local reports, or generated single-file builds.

The generated browser bundles and runtime files explicitly unignored in `.gitignore` are release assets for GitHub Pages. Refresh them with `bun run build` when their sources change; do not commit other `dist/` reports or single-file exports.

Changes to game rules, RNG, or planning can affect deterministic runs. Call those changes out explicitly and update fixtures only when the new outcome is intentional and verified.

## Assets and licensing

Only contribute material you can license under the repository terms. Code and documentation use MIT. Original artwork and media use CC0 1.0. Identify third party material and its license in `THIRD_PARTY_NOTICES.md`.
