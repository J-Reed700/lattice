# CI Ratchet: Immaculate Baseline Enforcement

**CI Ratchet** is our automated quality gate that enforces the **Immaculate Baseline** standards for production code. It ensures that only high-quality, panic-free code reaches our main branches.

## What is the Immaculate Baseline?

The Immaculate Baseline is our commitment to **production-grade Rust code** with three pillars:

1. **Zero Clippy Warnings** in production code (`--lib --bins`)
2. **All Tests Pass** (unit and integration tests)
3. **Zero-Panic Policy** (no `unwrap()` or `expect()` in production code)

### Why Production Code Only?

**Tests are ALLOWED to use `unwrap()` and `expect()`** because:
- Tests run in controlled environments
- Test failures are expected and caught
- `unwrap()` in tests makes them more readable
- Tests don't run in production

**Production code MUST NOT use `unwrap()` or `expect()`** because:
- Production code runs in user environments with unpredictable inputs
- Panics cause crashes and data loss
- Users deserve reliable software
- Proper error handling improves debugging

## Running Checks Locally

### Quick Check (Before Committing)

**ALWAYS run this before committing:**

```bash
just ci-quick
```

This runs:
- Production code clippy with zero-warning policy
- Test suite (unit and integration)

**Fast feedback** in ~30 seconds. Use this during development.

### Full CI Ratchet Check

```bash
just ci-check
# or
./scripts/ci_check.sh
```

This runs all three checks with detailed output:
1. Production code clippy (zero warnings)
2. Test suite
3. Zero-Panic Policy enforcement

Use this to verify your PR is ready before pushing.

### Other Useful Commands

```bash
# Production code lint only (fastest)
just lint-prod

# Comprehensive checks (format + lint + test + Zero-Panic)
just check-all

# Format check + CI check
just fmt-check && just ci-check
```

## CI Workflow

The **Immaculate CI Ratchet** GitHub Actions workflow runs on:
- All pushes to `main`, `develop`, and `feature/*` branches
- All pull requests to `main` and `develop`

### Jobs

#### 1. `immaculate-baseline` (Critical)

Three checks that MUST pass:

**[1/3] Production Code Clippy (Zero Warnings)**
```bash
cargo clippy --lib --bins -- -D warnings -D clippy::unwrap_used -D clippy::expect_used
```
- Fails on ANY clippy warning in production code
- Enforces `unwrap_used` and `expect_used` lints
- Only checks `--lib --bins` (excludes tests)

**[2/3] Test Suite**
```bash
cargo test --lib --bins
```
- Runs all unit and integration tests
- Fails if ANY test fails
- Tests production code functionality

**[3/3] Zero-Panic Policy (Production Code)**
```bash
rg '\.unwrap\(\)|\.expect\(' src/ -g '!*test*.rs' -c
```
- Counts `unwrap()` and `expect()` in production code
- Excludes test files (`*test*.rs`)
- Fails if count > 0
- Shows top offenders for easy fixing

#### 2. `build-check` (Verification)

Runs after `immaculate-baseline` passes:
```bash
cargo build --release
```
- Verifies release build succeeds
- Catches release-specific issues
- Runs with full optimizations

### Fast Feedback with Caching

The CI uses Rust caching (`Swatinem/rust-cache@v2`) for:
- Fast incremental builds (~2-5 minutes)
- Cached dependencies between runs
- Minimal download time

## Zero-Panic Policy

### What is it?

**Zero-Panic Policy**: Production code MUST NOT contain `unwrap()` or `expect()`.

### Why?

- **Panics are crashes** - they terminate the entire process
- **No recovery** - panics can't be caught at module boundaries
- **Data loss** - in-flight operations are lost
- **Poor UX** - users see crashes instead of error messages

### What to use instead?

❌ **Bad (Panic):**
```rust
let value = map.get(&key).unwrap();
```

✅ **Good (Graceful Error):**
```rust
let value = map.get(&key)
    .ok_or(AppError::KeyNotFound(key.clone()))?;
```

❌ **Bad (Panic):**
```rust
let result = risky_operation().expect("this should never fail");
```

✅ **Good (Graceful Error):**
```rust
let result = risky_operation()
    .map_err(|e| AppError::OperationFailed(e.to_string()))?;
```

### Exceptions

**Tests are ALLOWED to use `unwrap()` and `expect()`:**
```rust
#[test]
fn test_map_insert() {
    let mut map = HashMap::new();
    map.insert("key", "value");
    assert_eq!(map.get("key").unwrap(), &"value"); // OK in tests!
}
```

**Tests are excluded** from Zero-Panic Policy checks via `-g '!*test*.rs'` glob pattern.

## Installing Pre-commit Hook

**Recommended**: Install the pre-commit hook to catch issues before committing:

```bash
just install-hooks
```

This copies `scripts/pre-commit` to `.git/hooks/pre-commit` and makes it executable.

Now every commit will automatically run `just ci-quick`. If checks fail, the commit is blocked.

**To bypass** (use sparingly):
```bash
git commit --no-verify
```

## Troubleshooting

### "Production code clippy: FAIL"

**Fix clippy warnings** in `src/lib.rs`, `src/main.rs`, or `src/bin/`:
```bash
cargo clippy --lib --bins -- -D warnings
```

Read the warnings and fix them. Common issues:
- Unused imports or variables
- Unnecessary clones
- Missing `#[must_use]` attributes
- Inefficient code patterns

### "Tests: FAIL"

**Fix failing tests:**
```bash
cargo test --lib --bins
```

Read the test output and fix the failing tests. Common issues:
- Outdated test expectations
- Missing test data
- Race conditions in async tests
- Incorrect mocks

### "Zero-Panic Policy: FAIL"

**Find and replace `unwrap()` and `expect()`:**

1. **Find all occurrences** (excluding tests):
```bash
rg '\.unwrap\(\)|\.expect\(' src/ -g '!*test*.rs' -n
```

2. **Replace with proper error handling**:
   - Use `?` operator for functions returning `Result`
   - Use `ok_or()` or `ok_or_else()` to convert `Option` to `Result`
   - Use `map_err()` to wrap errors in custom error types

3. **Verify fix**:
```bash
just ci-check
```

### "CI is too slow"

**Use quick check during development:**
```bash
just ci-quick  # ~30 seconds
```

**Run full check before pushing:**
```bash
just ci-check  # ~2 minutes
```

**CI caching** reduces GitHub Actions time to ~2-5 minutes.

## FAQ

### Q: Why are tests excluded from Zero-Panic Policy?

**A:** Tests run in controlled environments and panics are expected. `unwrap()` in tests is idiomatic Rust and makes tests more readable.

### Q: Can I use `unwrap()` in private helper functions?

**A:** No. All production code (`--lib --bins`) must follow Zero-Panic Policy, regardless of visibility.

### Q: What if I need to panic for truly unrecoverable errors?

**A:** Use `panic!()` explicitly with a clear message. This documents the intentional panic and explains why it's unrecoverable. However, this should be extremely rare.

### Q: How do I test panic-free code?

**A:** Tests can use `unwrap()` freely:
```rust
#[test]
fn test_parsing() {
    let result = parse("input").unwrap();  // OK in tests!
    assert_eq!(result, expected);
}
```

### Q: Can I disable CI Ratchet for a specific PR?

**A:** No. CI Ratchet is **mandatory** for all PRs to `main` and `develop`. If you have a legitimate exception, discuss with the team first.

### Q: What's the difference between `ci-quick` and `ci-check`?

| Command | Checks | Time | Use Case |
|---------|--------|------|----------|
| `just ci-quick` | Clippy + Tests | ~30s | Before every commit |
| `just ci-check` | Clippy + Tests + Zero-Panic | ~2min | Before pushing PR |
| `just check-all` | Format + Clippy + Tests + Zero-Panic | ~3min | Final verification |

## Summary

**Before committing:**
```bash
just ci-quick
```

**Before pushing PR:**
```bash
just ci-check
```

**Install pre-commit hook:**
```bash
just install-hooks
```

**Remember:**
- ✅ `unwrap()` and `expect()` are OK in **tests**
- ❌ `unwrap()` and `expect()` are FORBIDDEN in **production code**
- ✅ Use `?` operator and proper error handling instead
- ✅ CI Ratchet is your friend - it catches bugs before users do

---

**Questions?** Check existing issues or ask in the team chat.

**Found a bug?** Open an issue with the error message and steps to reproduce.
