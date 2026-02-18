# Unwrap Lints Configuration

## Overview

This document describes the Clippy lint configuration to prevent `.unwrap()` and `.expect()` usage in production code while allowing them in tests.

## Configuration Files

### 1. Cargo.toml - Lint Rules

**Location**: `/vault/desktop/src-tauri/Cargo.toml`

```toml
[lints.clippy]
# Error handling - prevent panics in production
unwrap_used = "warn"           # Warn on .unwrap()
expect_used = "warn"           # Warn on .expect()
panic = "deny"                 # Deny explicit panic!()

# Additional safety lints
unwrap_in_result = "warn"      # Unwrap inside Result-returning functions
indexing_slicing = "warn"      # Array indexing without bounds check

# Code quality
missing_errors_doc = "warn"    # Document error cases
```

**Lint Levels**:
- `warn` - Shows warnings but doesn't fail build
- `deny` - Fails build on violations
- `allow` - Explicitly allows pattern

**Rationale**:
- `unwrap_used` / `expect_used` set to "warn" to catch new unwraps without breaking existing builds
- `panic` set to "deny" to prevent explicit panic calls
- Additional safety lints to catch related issues

### 2. .cargo/config.toml - Test Exceptions

**Location**: `/vault/desktop/src-tauri/.cargo/config.toml`

```toml
[target.'cfg(test)']
rustflags = [
    # Allow unwraps in tests
    "-A", "clippy::unwrap_used",
    "-A", "clippy::expect_used",
]
```

**Effect**:
- Test code (including `#[cfg(test)]` modules and `tests/` directory) can use unwraps
- Production code still gets warnings
- Applied automatically based on `cfg(test)` compilation flag

### 3. Justfile - Development Commands

**Location**: `/vault/desktop/src-tauri/Justfile`

```bash
# Check for unwraps in production code
check-unwraps:
    @echo "Checking for unwraps in production code..."
    @cargo clippy -- -D clippy::unwrap_used -D clippy::expect_used 2>&1 || true
    @echo "\nSearching source files (excluding tests):"
    @rg "\.unwrap\(\)" src/ -g '!*test*.rs' --count-matches 2>/dev/null | sort -t: -k2 -rn | head -20 || echo "No unwraps found!"
```

**Usage**:
```bash
just check-unwraps    # Check for unwraps
just lint             # Full lint check
just lint-prod        # Production code only
```

### 4. ERROR_HANDLING.md - Developer Guidelines

**Location**: `/vault/desktop/src-tauri/docs/ERROR_HANDLING.md`

Comprehensive guide covering:
- Prohibited patterns
- Recommended alternatives
- Use case examples
- Migration strategies

## How It Works

### During Development

```rust
// Production code (src/commands/my_feature.rs)
fn my_function() -> Result<String, AppError> {
    let value = option.unwrap();  // ⚠️ WARNING: clippy::unwrap_used
    //                               ^^^^^^^^^ help: use `?` or `ok_or`
    Ok(value)
}

// Test code (tests/my_test.rs or #[cfg(test)])
#[test]
fn test_my_function() {
    let value = option.unwrap();  // ✅ OK - allowed in tests
    assert_eq!(value, expected);
}
```

### Compilation Flow

1. **Cargo reads Cargo.toml** → Applies lints to all code
2. **For test targets** → `.cargo/config.toml` overrides with `-A` (allow)
3. **Clippy runs** → Warns on production unwraps, silent on test unwraps
4. **Build succeeds** → Warnings shown but don't fail build (yet)

## Enforcement Levels

### Current (Phase 1): Warn

```toml
unwrap_used = "warn"
expect_used = "warn"
```

- Warnings shown during compilation
- Build succeeds
- Prevents accidental introduction
- Allows gradual migration

### Future (Phase 2): Deny

```toml
unwrap_used = "deny"
expect_used = "deny"
```

- Errors shown during compilation
- Build fails on violations
- Strict enforcement
- Deploy after cleanup complete

## Checking for Violations

### Command Line

```bash
# Check production code only (denies unwraps)
cargo clippy --lib --bins -- -D clippy::unwrap_used -D clippy::expect_used

# Check all code (including tests)
cargo clippy --all-targets

# Search for unwraps in source (excluding tests)
rg "\.unwrap\(\)" src/ -g '!*test*.rs'

# Count unwraps by file
rg "\.unwrap\(\)" src/ -g '!*test*.rs' --count-matches | sort -t: -k2 -rn
```

### Using Justfile

```bash
just check-unwraps    # Runs clippy + ripgrep search
just lint             # Full lint with all warnings as errors
just lint-prod        # Production code only
```

### CI/CD Integration

Add to `.github/workflows/rust.yml`:

```yaml
- name: Clippy check
  run: |
    cd vault/desktop/src-tauri
    cargo clippy --all-targets -- -D clippy::unwrap_used -D clippy::expect_used
```

## Migration Path

### Phase 1: Enable Warnings (Current)

- [x] Add `[lints.clippy]` section to Cargo.toml
- [x] Configure test exceptions in .cargo/config.toml
- [x] Create ERROR_HANDLING.md guide
- [x] Add Justfile commands
- [ ] Monitor warnings during development
- [ ] Fix new unwraps as they appear

### Phase 2: Cleanup Existing (In Progress)

- [ ] Replace existing unwraps with proper error handling
- [ ] Add audit logging where appropriate
- [ ] Add tests for error paths
- [ ] Document error cases

### Phase 3: Strict Enforcement (Future)

- [ ] Change `warn` → `deny` in Cargo.toml
- [ ] Add CI/CD checks
- [ ] Update contribution guidelines
- [ ] Reject PRs with unwraps

## Benefits

### Developer Experience

✅ **Early Detection**: Warns during `cargo check` / `cargo build`
✅ **IDE Integration**: Works with rust-analyzer in VSCode/IntelliJ
✅ **Fast Feedback**: No need to wait for clippy run
✅ **Clear Messages**: Explains why unwrap is problematic

### Code Quality

✅ **Prevent Panics**: Catch potential runtime panics at compile time
✅ **Better Errors**: Forces meaningful error messages
✅ **Audit Trail**: Errors can be logged for security compliance
✅ **Type Safety**: Encourages Result/Option handling

### Testing

✅ **Pragmatic**: Tests can still use unwrap for simplicity
✅ **Clear Intent**: Test panics are expected behavior
✅ **Fast Tests**: No overhead from error handling
✅ **Debugging**: Stack traces from unwrap aid test debugging

## Troubleshooting

### "Still seeing unwraps in test code warnings"

**Solution**: Ensure you're using `cargo test` (not `cargo clippy --all-targets`):
```bash
cargo test           # ✅ Tests exempt from unwrap lints
cargo clippy --tests # ⚠️ May show warnings depending on config
```

### "Build failing on deny lints"

**Solution**: Either:
1. Fix the violation (recommended)
2. Temporarily allow specific instance:
   ```rust
   #[allow(clippy::unwrap_used)]
   let value = option.unwrap();
   ```

### "Lints not showing in IDE"

**Solution**: Restart rust-analyzer:
```bash
# VSCode: Cmd+Shift+P → "Rust Analyzer: Restart Server"
# IntelliJ: Settings → Languages → Rust → Restart Analyzer
```

## Examples

See `ERROR_HANDLING.md` for comprehensive examples of:
- Option handling
- Result handling
- Lock handling
- File I/O
- Database operations
- Async operations
- Migration strategies

## Related Documentation

- [ERROR_HANDLING.md](./ERROR_HANDLING.md) - Comprehensive error handling guide
- [ARCHITECTURE.md](../ARCHITECTURE.md) - Domain-driven design patterns
- [DEVELOPER_GUIDE.md](../DEVELOPER_GUIDE.md) - Development workflows

## References

- [Clippy Lints Documentation](https://rust-lang.github.io/rust-clippy/master/)
- [Cargo Lints](https://doc.rust-lang.org/cargo/reference/manifest.html#the-lints-section)
- [Rust Error Handling](https://doc.rust-lang.org/book/ch09-00-error-handling.html)
