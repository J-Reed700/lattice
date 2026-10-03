# api-rust (Sync Service Scaffold)

Rust scaffold for Lattice's optional sync backend. The desktop app does not
call it yet.

## What is implemented

- `axum` HTTP service with:
  - `GET /healthz`
  - `POST /v1/sync/devices/register`
  - `POST /v1/sync/push`
  - `POST /v1/sync/pull`
  - `POST /v1/sync/ack`
  - `POST /v1/sync/conflicts/resolve`
  - `GET /v1/sync/status/{device_id}`
- Layered modules: HTTP routes, sync service, repository, persistence.
- PostgreSQL schema/migration for:
  - `devices`
  - `document_heads`
  - `document_ops` (append-only operation log)
  - `device_checkpoints`
  - `conflicts`
  - `outbox`
- Idempotent write key: `(user_id, device_id, client_op_id)`.
- Sequence-cursor pull (`since_seq`) instead of timestamp cursor.

## Local run

1. Copy env file:

```bash
cp .env.example .env
```

2. Generate and set a private bearer token, and configure the tenant identity
   that this one token represents:

```bash
openssl rand -hex 32
```

Put the output in `API_BEARER_TOKEN` and set `API_USER_ID` to the existing
server-side tenant/user ID. The API rejects startup if the token is absent or
shorter than 32 bytes, or if the user ID is not positive. Sync requests must
send `Authorization: Bearer <token>`. The service derives the tenant from this
server configuration and ignores `x-user-id`; do not share the token with
untrusted clients. This scaffold configures one principal and is not a
multi-tenant identity provider.

3. Ensure Postgres is running and `DATABASE_URL` points to it.

`HOST` defaults to `127.0.0.1` and `PORT` to `8080`. `MAX_DB_CONNECTIONS`
(default `20`) sizes the pool, and `RUN_MIGRATIONS` (default on) applies
`migrations/` at startup. Set `CORS_ALLOWED_ORIGINS` to a comma-separated
list of exact application origins when needed; wildcard origins are rejected.
The example lists the local development and Tauri origins. Binding to a
non-loopback address is an explicit deployment choice and should be paired
with TLS termination and network controls.

4. Start service:

```bash
cargo run
```

## Current limitations (intentional for scaffold)

- The configured opaque bearer token maps to one server-configured tenant.
  Use an established identity provider and verified tenant claims before
  serving multiple independent users.
- Conflict detection is currently base-version mismatch only.
- Conflict resolution does not yet merge document content into `document_heads`.
- Outbox publisher worker is not implemented yet (rows are only written).
- No gRPC stream yet (`tonic` can be added once pull/push behavior stabilizes).

## Suggested next steps

1. Extend PostgreSQL contract coverage as conflict and delivery semantics grow.
2. Implement full conflict resolution semantics and merged head writes.
3. Add background outbox dispatcher and delivery retries.
4. Replace the single configured principal with verified identity-provider
   tokens and tenant claims before multi-tenant deployment.
5. Add metrics/tracing spans per sync request and DB transaction.

## Verification

`cargo test --lib` runs database-free service and HTTP-error contract tests.
`cargo test --test sync_persistence -- --ignored` runs PostgreSQL contracts when
`DATABASE_URL` points to a disposable test server. SQLx creates isolated test
databases. These tests cover rollback, idempotent retry, tenant isolation,
monotonic acknowledgements and concurrent first-write conflict detection.
CI runs both suites with PostgreSQL 16.

Push transactions serialize per user with a transaction-scoped advisory lock.
This protects absent document heads and same-user operation commit order without
blocking pushes from other users. It deliberately trades same-user parallelism
for correctness; do not remove it without replacing both guarantees.
