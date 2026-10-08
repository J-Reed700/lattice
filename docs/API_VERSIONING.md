# API and IPC versioning

**Last updated:** 2026-10-07

Lattice currently has two programmatic surfaces with different compatibility
boundaries:

1. the desktop application's Tauri IPC contract, which ships with its renderer;
2. the optional Rust sync-service scaffold in `api-rust/`, which exposes HTTP
   routes under `/v1/sync` but is not connected to the desktop application.

Neither surface is a supported third-party plugin API.

## Desktop IPC

The renderer calls Rust through Tauri commands. The frontend and backend ship in
the same application build and change together, so this internal contract does
not have a URL or semantic API version.

`src/lib/bindings.ts` is generated from the Rust command types:

```bash
npm run bindings:generate
npm run bindings:check
```

The repository also checks the hand-maintained IPC wrappers and command
registration:

```bash
npm run contracts:check
npm run contracts:commands
```

A command change must update its Rust handler, plugin registration, Tauri
permission, binding export inventory, generated binding, and frontend call site
in the same change. The detailed checklist is in
[Function Calling Implementation Guide](FUNCTION_CALLING_IMPLEMENTATION_GUIDE.md#adding-or-changing-an-ipc-command).

Because Lattice is pre-release, old desktop builds are not promised
compatibility with a newer backend or renderer. A released application always
uses the IPC implementation bundled with that release.

## Sync HTTP scaffold

[`api-rust/`](../api-rust/README.md) contains an implemented Axum service
scaffold. The desktop application does not call it yet. Its current routes are:

- `GET /healthz`
- `POST /v1/sync/devices/register`
- `POST /v1/sync/push`
- `POST /v1/sync/pull`
- `POST /v1/sync/ack`
- `POST /v1/sync/conflicts/resolve`
- `GET /v1/sync/status/{device_id}`

The route registration is the source of truth:
[`api-rust/src/http/mod.rs`](../api-rust/src/http/mod.rs) and
[`api-rust/src/http/routes/sync.rs`](../api-rust/src/http/routes/sync.rs).

`v1` is the current route namespace, not a production stability or support
promise. While the service remains a disconnected scaffold, request and response
shapes may change without a deprecation window. Any such change must update the
route types, service and persistence tests, and `api-rust/README.md` together.

Once the desktop app or an external client depends on the service, incompatible
changes require a new major route namespace. Removing or renaming routes,
changing required fields or field types, changing authentication semantics, or
changing the meaning of an existing response counts as incompatible. Additive
optional fields and new routes may remain in the same major version when old
clients can safely ignore them.

The scaffold currently uses one server-configured bearer token and tenant. It
does not implement version-discovery headers, deprecation or sunset headers, a
plugin API, multi-tenant identity-provider authentication, or a published
support/EOL schedule.

## Verification

Desktop contract checks run from the repository root:

```bash
npm run bindings:check
npm run contracts:check
npm run contracts:commands
```

Sync service checks run from its crate:

```bash
cd api-rust
cargo test --lib
```

PostgreSQL persistence contracts are opt-in because they require a disposable
database:

```bash
cd api-rust
cargo test --test sync_persistence -- --ignored
```
