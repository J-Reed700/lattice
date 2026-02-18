# CI Architecture Quality Checks

**Phase 6b Implementation Complete** ✅

## Overview

Comprehensive CI checks to enforce architecture quality and prevent test code contamination in production builds.

## What Was Added

### 1. GitHub Actions Workflow
**File**: `.github/workflows/architecture-checks.yml`

A comprehensive workflow with 6 parallel jobs:
- Mock Isolation Check
- Production Build Check
- Clippy Architecture Lints
- Documentation Check
- Test Coverage Analysis
- Feature Combination Check

### 2. Cargo.toml Profiles
**File**: `vault/desktop/src-tauri/Cargo.toml`

Added explicit test and benchmark profiles:
```toml
[profile.test]
inherits = "dev"

[profile.bench]
inherits = "release"
debug = true
```

### 3. Local Validation Script
**File**: `vault/desktop/src-tauri/scripts/check_architecture.sh`

Executable script to run all checks locally before pushing:
```bash
./scripts/check_architecture.sh
```

### 4. Documentation
- `.github/workflows/README.md` - Comprehensive workflow documentation
- `vault/desktop/src-tauri/docs/CI_ARCHITECTURE_CHECKS.md` - This file

## Checks Performed

### ✅ Mock Isolation Check
**What**: Ensures all mock implementations are `#[cfg(test)]` guarded

**Why**: Prevents mock code from being compiled into production binaries

**How**:
- Finds all `mock_*.rs` and `*/mocks/*.rs` files
- Verifies each has `#[cfg(test)]` attribute
- Checks production code doesn't import mocks

**Failure Criteria**:
- Mock file without `#[cfg(test)]`
- Production code imports mock modules

**Example Fix**:
```rust
#[cfg(test)]
pub mod mock_search {
    // mock implementation
}
```

### ✅ Production Build Check
**What**: Verifies release builds exclude test code and dependencies

**Why**: Ensures production binaries are optimized and secure

**How**:
- Runs `cargo build --release --lib`
- Checks `cargo tree` for test dependencies
- Analyzes binary for mock symbols

**Failure Criteria**:
- Release build fails
- Test dependencies (mockall, proptest, criterion) in production
- Binary contains mock symbols

**Example Fix**:
```toml
# Move test deps to [dev-dependencies]
[dev-dependencies]
mockall = "0.13"
```

### ✅ Clippy Architecture Lints
**What**: Strict Clippy rules for architecture quality

**Why**: Prevents common bugs and enforces best practices

**Enforced Rules**:
```bash
-D clippy::unwrap-used       # No .unwrap()
-D clippy::expect-used       # No .expect()
-D clippy::panic             # No panic!()
-D clippy::todo              # No todo!()
-D clippy::unimplemented     # No unimplemented!()
-D clippy::dbg-macro         # No dbg!()
-W clippy::cognitive-complexity
-W clippy::too-many-arguments
-W clippy::type-complexity
```

**Failure Criteria**:
- Any use of unwrap/expect/panic in production code
- Todo or unimplemented markers
- Debug macros (dbg!)

**Example Fixes**:
```rust
// ❌ Bad
let value = result.unwrap();

// ✅ Good
let value = result.map_err(|e| AppError::from(e))?;

// ❌ Bad
panic!("Something went wrong");

// ✅ Good
return Err(AppError::InternalError("Something went wrong".into()));
```

### ✅ Documentation Check
**What**: Verifies architecture documentation exists and is complete

**Why**: Ensures maintainability and onboarding

**Required**:
- `ARCHITECTURE.md` must exist
- Must contain sections:
  - Domain-Driven Design
  - SOLID Principles
  - Service Container
  - Dependency Injection
  - Security

**Failure Criteria**:
- ARCHITECTURE.md missing
- Required sections incomplete

### ✅ Test Coverage Analysis
**What**: Runs cargo-tarpaulin for coverage metrics

**Why**: Visibility into test completeness

**How**:
- Generates XML and HTML reports
- Currently informational (no hard threshold)

**Future**: Set minimum coverage threshold (e.g., 70%)

### ✅ Feature Combination Check
**What**: Tests all feature flag combinations compile

**Why**: Ensures feature flags work correctly

**Combinations Tested**:
- Default features
- No default features
- Each feature individually (search, indexing, qa, extraction)

**Failure Criteria**:
- Any feature combination doesn't compile

## Workflow Triggers

**Pull Requests**:
```yaml
paths:
  - 'vault/desktop/src-tauri/**'
```

**Push to Main/Master/Develop**:
```yaml
branches:
  - main
  - master
  - develop
```

## Performance

**Typical Run Times** (with cache):
- Mock Isolation: ~30 seconds
- Production Build: ~2-5 minutes
- Clippy: ~3-5 minutes
- Documentation: ~10 seconds
- Coverage: ~5-10 minutes
- Feature Combinations: ~5-10 minutes

**Total**: ~15-25 minutes

**First Run** (no cache): ~30-45 minutes

## Caching Strategy

Multiple cache layers for faster builds:

1. **Cargo Registry Cache**
   - Path: `~/.cargo/registry`
   - Key: OS + Cargo.lock hash

2. **Cargo Git Cache**
   - Path: `~/.cargo/git`
   - Key: OS + Cargo.lock hash

3. **Build Artifacts Cache**
   - Path: `vault/desktop/src-tauri/target`
   - Key: OS + job + Cargo.lock hash

## Local Validation

Before pushing, run:

```bash
cd vault/desktop/src-tauri
./scripts/check_architecture.sh
```

Or manually:

```bash
# 1. Mock isolation
find src -type f -name "mock_*.rs" -exec grep -L "#\[cfg(test)\]" {} \;

# 2. Production build
cargo build --release --lib

# 3. Clippy
cargo clippy --all-targets --all-features -- -D warnings

# 4. Coverage
cargo tarpaulin --verbose --all-features --workspace

# 5. Features
cargo check --release
cargo check --release --no-default-features
cargo check --release --no-default-features --features search
```

## Integration with Development Workflow

### Pre-Commit
Add to `.git/hooks/pre-commit`:
```bash
#!/bin/bash
cd vault/desktop/src-tauri
./scripts/check_architecture.sh
```

### IDE Integration
**VS Code** - Add to `.vscode/tasks.json`:
```json
{
  "label": "Check Architecture",
  "type": "shell",
  "command": "./scripts/check_architecture.sh",
  "options": {
    "cwd": "${workspaceFolder}/vault/desktop/src-tauri"
  }
}
```

## Interpreting Results

### ✅ All Checks Passed (Green)
- Code meets architecture quality standards
- Safe to merge

### ⚠️ Warnings (Yellow)
- Non-critical issues
- Review and address if possible
- Examples:
  - Coverage below recommended threshold
  - Documentation incomplete
  - Code complexity warnings

### ❌ Checks Failed (Red)
- Critical architecture violations
- Must fix before merge
- Examples:
  - Mock contamination
  - Unwrap in production code
  - Build failure
  - Test dependencies in release

## Common Issues and Fixes

### Issue: "Mock files missing cfg(test)"
**Symptom**: Mock Isolation Check fails

**Fix**:
```rust
// Add to top of mock file
#[cfg(test)]
pub mod mock_my_service {
    use super::*;
    // mock implementation
}
```

### Issue: "Release build includes test dependencies"
**Symptom**: Production Build Check fails

**Fix**:
```toml
# Move to [dev-dependencies]
[dev-dependencies]
mockall = "0.13"
proptest = "1.5"
criterion = "0.5"
```

### Issue: "Clippy unwrap-used error"
**Symptom**: Clippy Architecture Lints fails

**Fix**:
```rust
// ❌ Bad
let value = result.unwrap();

// ✅ Good
let value = result.map_err(|e| AppError::from(e))?;

// Or with context
let value = result
    .map_err(|e| AppError::InvalidInput(format!("Failed: {}", e)))?;
```

### Issue: "Feature combination doesn't compile"
**Symptom**: Feature Combination Check fails

**Fix**:
```toml
# Ensure features have correct dependencies
[features]
default = ["search", "indexing"]
search = ["dep:fastembed"]  # Declare optional deps
indexing = ["dep:tokenizers"]
```

## Security Benefits

### 1. Mock Contamination Prevention
- No test code in production = smaller attack surface
- No mock implementations that could bypass security checks
- No test credentials or keys in production binary

### 2. Strict Error Handling
- No unwrap = no panic on error
- No expect = no panic with custom message
- Explicit error propagation = better error handling

### 3. Code Quality
- Reduced cognitive complexity = easier security audits
- Fewer arguments = simpler interfaces = fewer bugs
- Type safety = compile-time guarantees

## Maintenance

### Adding New Checks
1. Add job to `architecture-checks.yml`
2. Add to `needs` array in `summary` job
3. Add to local script `check_architecture.sh`
4. Update documentation
5. Test locally first

### Adjusting Strictness

**More Strict**:
```yaml
# Add more Clippy lints
-D clippy::missing-docs-in-private-items
-D clippy::cargo-common-metadata
```

**Less Strict**:
```yaml
# Allow specific patterns
-A clippy::unwrap-used  # Allow unwrap (not recommended)
```

### Setting Coverage Threshold

In `architecture-checks.yml`, add:
```yaml
- name: Check coverage threshold
  run: |
    COVERAGE=$(cargo tarpaulin --output-format json | jq '.coverage')
    if (( $(echo "$COVERAGE < 70.0" | bc -l) )); then
      echo "Coverage $COVERAGE% below 70% threshold"
      exit 1
    fi
```

## Future Enhancements

### Planned
- [ ] Minimum coverage threshold (70%)
- [ ] Binary size regression detection
- [ ] Dependency audit (cargo-audit)
- [ ] Security vulnerability scanning (cargo-deny)
- [ ] SBOM generation
- [ ] ADR validation

### Potential
- [ ] Performance benchmark regression tests
- [ ] Memory leak detection (valgrind)
- [ ] Fuzz testing integration
- [ ] License compliance check
- [ ] API breaking change detection

## Related Documentation

- `/vault/desktop/ARCHITECTURE.md` - Architecture overview
- `/vault/desktop/DEVELOPER_GUIDE.md` - Development guide
- `/.github/workflows/README.md` - Workflow documentation
- `/CLAUDE.md` - AI assistant guide

## Compliance

These checks help ensure compliance with:
- **OWASP ASVS** - Application Security Verification Standard
- **CWE-778** - Insufficient Logging
- **CWE-770** - Resource Exhaustion
- **CWE-22** - Path Traversal (via input validation)
- **SOLID Principles** - Software design best practices
- **DDD** - Domain-Driven Design patterns

## Questions?

See:
- Workflow README: `.github/workflows/README.md`
- Local script: `scripts/check_architecture.sh`
- GitHub Actions logs: https://github.com/YOUR_ORG/Recall/actions

## Version History

- **v1.0.0** (2024-01-14): Initial implementation
  - 6 parallel checks
  - Local validation script
  - Comprehensive documentation
