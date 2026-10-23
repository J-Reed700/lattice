# Git hooks

Turn them on once per clone (worktrees share the setting):

```sh
git config core.hooksPath .githooks
```

| Hook | Runs | On |
|---|---|---|
| `pre-commit` | `rustfmt --check` on staged `.rs` files; `eslint --max-warnings 0` on staged `src/`/`e2e/` TS files and `tsc --noEmit` | every commit |
| `pre-push` | for what the push changes against `origin/main`: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --lib` for each touched `features::<name>` (and `lattice-core` when it changed); `type-check`, `type-check:e2e`, `lint`, `contracts:check` and `vitest run --coverage` when frontend files changed; `npm audit --audit-level=high` when `package.json` or the lockfile changed | every push |

CI still runs the full suite: every Rust test, bindings, the architecture checks, desktop and e2e.

`CARGO_HOOK_WRAPPER` puts a command before `cargo` in `pre-push`, for example a build lock on a machine that runs several builds at once.
