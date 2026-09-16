# Lattice desktop app

This directory contains the Tauri desktop application:

- `websrc/` contains the React and TypeScript frontend.
- `src/` contains the Rust crate and Tauri configuration.
- `e2e/` contains Playwright smoke tests.

## Development

From this directory:

```bash
npm ci
npm run tauri:dev
```

Useful checks:

```bash
npm run type-check
npm run lint
npm test -- --run
npm run test:e2e
cd src && cargo test --lib
```

Run `npm run bindings:generate` after changing a Rust type or command exported
through the IPC boundary. Commit the generated `websrc/lib/bindings.ts` update
with the Rust change.

Build an installer with `npm run tauri:build`. Platform packaging may require
additional signing configuration; local macOS builds use ad-hoc signing.

Project-wide setup and architecture conventions are documented in the
[root README](../README.md) and [contribution guide](../CONTRIBUTING.md).
