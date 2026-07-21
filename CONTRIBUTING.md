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

Only contribute code and assets you have the right to share. Describe the source and license of new visual or audio assets in the pull request. The repository owner is responsible for choosing the project license and maintaining attribution files.
