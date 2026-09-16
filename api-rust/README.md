# api-rust (Sync Service Scaffold)

Rust scaffold for Recall's sync backend.

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

2. Ensure Postgres is running and `DATABASE_URL` points to it.

3. Start service:

```bash
cargo run
```

## Current limitations (intentional for scaffold)

- Auth is placeholder (`x-user-id` header).
- Conflict detection is currently base-version mismatch only.
- Conflict resolution does not yet merge document content into `document_heads`.
- Outbox publisher worker is not implemented yet (rows are only written).
- No gRPC stream yet (`tonic` can be added once pull/push behavior stabilizes).

## Suggested next steps

1. Extend PostgreSQL contract coverage as conflict and delivery semantics grow.
2. Implement full conflict resolution semantics and merged head writes.
3. Add background outbox dispatcher and delivery retries.
4. Add JWT auth middleware and tenant/user claims extraction.
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
