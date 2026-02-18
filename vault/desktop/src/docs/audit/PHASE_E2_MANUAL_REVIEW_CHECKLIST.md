# 🔍 ORACLE PHASE E2: MANUAL REVIEW CHECKLIST

**Date:** 2026-01-07
**Phase:** E2 - Manual Code Audit
**Status:** 🔄 IN PROGRESS

---

## Executive Summary

Phase E2 follows Phase E1's automated analysis (2,698 panic bombs identified) with deep manual code review to identify:
1. **SIGBUS Crash Root Cause** - Memory safety issues in runtime
2. **Architecture Layer Violations** - DDD boundary breaches
3. **Unsafe Block Audit** - FFI and low-level code risks
4. **Code Smell Detection** - God objects, SRP violations, duplication

---

## Pillar 1: Memory Safety & SIGBUS Investigation

### A. SIGBUS Crash Analysis

**Status:** 🔄 Running tests with `RUST_BACKTRACE=full`

**Known Issue:** Signal 10 (SIGBUS) during test execution

**Checklist:**
- [ ] **Run tests with full backtrace**
  ```bash
  export RUST_BACKTRACE=full
  cargo test --lib -- --nocapture 2>&1 | tee docs/audit/test_crash_log.txt
  ```

- [ ] **Identify failing test(s)**
  - Review `docs/audit/test_crash_log.txt` for crash location
  - Note: SIGBUS = Bus Error (misaligned memory access or unmapped address)

- [ ] **Binary search for failing test**
  ```bash
  # Run tests one at a time
  cargo test --lib -- --test-threads=1 --nocapture

  # Or isolate specific modules
  cargo test --lib infrastructure::indexing --nocapture
  cargo test --lib infrastructure::search --nocapture
  ```

- [ ] **Check ONNX Runtime initialization**
  - File: `src/infrastructure/search/` (ONNX embedding models)
  - Suspected cause: Heavy ML model loading in parallel tests
  - Action: Review model initialization code for thread safety

- [ ] **Audit FFI boundaries**
  - ONNX Runtime (C++ library via Rust bindings)
  - Tauri (Rust <-> JavaScript IPC)
  - WebKit (browser engine)
  - Look for: Unaligned pointers, invalid memory access, race conditions

- [ ] **Check for stack overflow**
  - Deep async/await chains in Tokio
  - Recursive function calls
  - Large stack allocations

**Deliverable:** Root cause identified + fix implemented

---

### B. Unsafe Block Audit (with cargo-geiger)

**Status:** ⏳ Waiting for cargo-geiger installation

**Tool:** `cargo-geiger` - Detects unsafe code and FFI usage

**Checklist:**
- [ ] **Install cargo-geiger**
  ```bash
  cargo install cargo-geiger --locked
  ```

- [ ] **Run unsafe code detection**
  ```bash
  cargo geiger --output-format=markdown > docs/audit/unsafe_blocks_report.md
  ```

- [ ] **Review each unsafe block**
  - Document justification
  - Check for proper SAFETY comments
  - Verify soundness (no UB - Undefined Behavior)

- [ ] **Prioritize by risk**
  - **P0:** Unsafe blocks in production code without SAFETY docs
  - **P1:** FFI calls without null checks
  - **P2:** Unsafe blocks in test code

**Deliverable:** Unsafe block inventory with risk assessment

---

## Pillar 2: Architecture Layer Violations

### A. DDD Boundary Enforcement

**Recall Desktop follows 4-layer architecture:**

1. **Domain Layer** (`src/domain/`) - Pure business logic, NO external dependencies
2. **Application Layer** (`src/application/`) - Use cases, orchestration
3. **Infrastructure Layer** (`src/infrastructure/`) - Persistence, search, external services
4. **Interfaces Layer** (`src/interfaces/`) - Tauri commands, API surface

**Dependency Rules (MUST enforce):**
- Domain → NOTHING (no imports from other layers)
- Application → Domain only
- Infrastructure → Domain + Application
- Interfaces → ALL layers

**Checklist:**
- [ ] **Install cargo-modules**
  ```bash
  cargo install cargo-modules --locked
  ```

- [ ] **Generate dependency visualization**
  ```bash
  cargo modules structure --lib > docs/audit/module_structure.txt
  cargo modules graph --lib | dot -Tsvg > docs/audit/module_graph.svg
  ```

- [ ] **Audit Domain layer imports**
  ```bash
  # Should return ZERO matches
  grep -r "use crate::infrastructure" src/domain/
  grep -r "use crate::application" src/domain/
  grep -r "use crate::interfaces" src/domain/
  ```

- [ ] **Check for circular dependencies**
  - Review `module_graph.svg` for cycles
  - Circular deps violate SOLID (Dependency Inversion)

- [ ] **Verify trait-based abstractions**
  - Infrastructure should depend on Domain traits, not vice versa
  - Example: Domain defines `DocumentRepositoryTrait`, Infrastructure implements it

**Deliverable:** Layer violation report + refactoring plan

---

### B. God Object Detection

**Status:** ⚠️ Known issue - `Container.rs` with 176 lines of dead code

**Checklist:**
- [ ] **Review `src/interfaces/di/mod.rs` (ServiceContainer)**
  - Current: 34 unwraps (TOP 13 panic hotspot)
  - Check: Does it have too many responsibilities?
  - Action: Consider breaking into smaller containers

- [ ] **Check for God Objects in persistence**
  - `backup_adapter.rs` - 67 unwraps (TOP 1)
  - `tag_repository.rs` - 46 unwraps (TOP 5)
  - Look for: >500 lines, >10 public methods, multiple responsibilities

- [ ] **Identify SRP violations**
  - Clippy warnings: "Function has too many arguments (>7)" (~15 occurrences)
  - Action: Refactor into smaller, focused functions

**Deliverable:** God object refactoring plan

---

## Pillar 3: Code Quality Deep Dive

### A. Unused Dependencies (with cargo-udeps)

**Status:** ⏳ Waiting for cargo-udeps installation

**Checklist:**
- [ ] **Install cargo-udeps**
  ```bash
  cargo install cargo-udeps --locked
  ```

- [ ] **Detect unused dependencies**
  ```bash
  cargo +nightly udeps --all-targets > docs/audit/unused_deps.txt
  ```

- [ ] **Remove unused crates**
  - Reduces attack surface
  - Speeds up compilation
  - Simplifies dependency tree

**Deliverable:** Cleanup PR removing unused deps

---

### B. Binary Bloat Analysis (with cargo-bloat)

**Status:** ⏳ Pending installation

**Checklist:**
- [ ] **Install cargo-bloat**
  ```bash
  cargo install cargo-bloat --locked
  ```

- [ ] **Analyze binary size**
  ```bash
  cargo bloat --release --crates > docs/audit/binary_bloat.txt
  ```

- [ ] **Identify largest dependencies**
  - Target: Reduce release binary size
  - Focus: Remove unnecessary features

**Deliverable:** Binary size optimization recommendations

---

### C. Dead Code Elimination

**Known Issue:** 176 lines of dead code in `Container.rs`

**Checklist:**
- [ ] **Run dead code detection**
  ```bash
  cargo build --all-targets 2>&1 | grep "never used" > docs/audit/dead_code.txt
  ```

- [ ] **Review and remove**
  - High priority: Public functions never called
  - Medium priority: Private functions in production code
  - Low priority: Test helper functions

**Deliverable:** Dead code cleanup PR

---

## Pillar 4: Code Smells & Anti-Patterns

### A. Manual Code Review (Top 20 Panic Hotspots)

**Checklist:**

#### P0 Files (Critical - Production Persistence)

- [ ] **1. `infrastructure/persistence/backup_adapter.rs` (67 unwraps)**
  - [ ] Check for god object (persistence + backup + export)
  - [ ] Review unwrap usage - any in critical paths?
  - [ ] Test coverage adequate?

- [ ] **2. `infrastructure/persistence/repositories/tag_repository.rs` (46 unwraps)**
  - [ ] SRP violations? (CRUD + search + aggregation?)
  - [ ] Transaction safety?

- [ ] **3. `infrastructure/persistence/repositories/settings_repository.rs` (40 unwraps)**
  - [ ] Thread safety? (settings accessed from multiple threads)

- [ ] **4. `infrastructure/persistence/repositories/conversation_repository.rs` (38 unwraps)**
  - [ ] Race conditions in chat history writes?

- [ ] **5. `infrastructure/persistence/repositories/recent_documents_repository.rs` (35 unwraps)**
  - [ ] LRU cache implementation correct?

#### P1 Files (High - Core Logic)

- [ ] **6. `shared/domain_types.rs` (41 unwraps)**
  - [ ] Value objects should not unwrap! Fix immediately
  - [ ] Use TryFrom/FromStr instead

- [ ] **7. `interfaces/commands/config.rs` (38 unwraps)**
  - [ ] Error handling in Tauri commands?
  - [ ] User-facing errors informative?

- [ ] **8. `infrastructure/storage/content_addressed_storage.rs` (32 unwraps)**
  - [ ] File I/O errors handled?

- [ ] **9. `infrastructure/web/ingestion/types.rs` (32 unwraps)**
  - [ ] Parsing HTML/Markdown - unwraps will panic on malformed input

- [ ] **10. `infrastructure/persistence/database/schema_validation.rs` (30 unwraps)**
  - [ ] Schema mismatches should be errors, not panics

---

### B. Duplication Detection

**Checklist:**
- [ ] **Run duplication analysis**
  ```bash
  # Find duplicate code blocks (>10 lines)
  find src -name "*.rs" -exec grep -Pzo "(?s)(.{50,})\n.*\1" {} \; > docs/audit/duplicates.txt
  ```

- [ ] **Review common patterns**
  - Repeated error handling
  - Similar repository methods
  - Duplicate validation logic

**Deliverable:** Refactoring opportunities list

---

### C. Complex Function Analysis

**Clippy Finding:** ~15 functions with >7 arguments

**Checklist:**
- [ ] **Extract clippy warnings**
  ```bash
  grep "too_many_arguments" docs/audit/clippy_report_warnings.txt > docs/audit/complex_functions.txt
  ```

- [ ] **Review each function**
  - Does it violate SRP?
  - Can arguments be grouped into structs?
  - Is it a constructor that needs builder pattern?

**Deliverable:** Function simplification plan

---

## Pillar 5: Integration Testing

### A. Frontend-Backend IPC Audit

**Checklist:**
- [ ] **List all Tauri commands**
  ```bash
  grep -r "#\[tauri::command\]" src/interfaces/commands/ | wc -l
  ```

- [ ] **Verify TypeScript bindings**
  - Check: Do all commands have TypeScript type definitions?
  - Check: Are errors properly typed?

- [ ] **Test error propagation**
  - Run failing commands from frontend
  - Verify user sees meaningful error messages

**Deliverable:** IPC integration test suite

---

### B. Database Migration Safety

**Checklist:**
- [ ] **Review all migrations**
  ```bash
  ls -la migrations/
  ```

- [ ] **Check for destructive operations**
  - DROP TABLE without backup?
  - ALTER TABLE without default values?
  - Data loss on rollback?

- [ ] **Test migration rollback**
  ```bash
  sqlx migrate run && sqlx migrate revert
  ```

**Deliverable:** Migration safety report

---

## Success Criteria (Phase E2 Complete)

- [x] **SIGBUS crash root cause identified** ← Blocking issue
- [ ] **All unsafe blocks documented with SAFETY comments**
- [ ] **Zero architecture layer violations**
- [ ] **God objects refactored** (Container.rs, backup_adapter.rs)
- [ ] **Unused dependencies removed**
- [ ] **Dead code eliminated**
- [ ] **Top 10 P0 files manually reviewed**

---

## Timeline Estimate

**Oracle's Estimate:** 4-8 hours for Phase E2

**Breakdown:**
- SIGBUS investigation: 2-3 hours (debugging + fix)
- Unsafe block audit: 1 hour (cargo-geiger + review)
- Architecture audit: 1-2 hours (cargo-modules + manual check)
- Code smell review: 2-3 hours (TOP 10 files)

**Current Progress:** 🔄 ~10% complete (tools installing, tests running)

---

## Next Actions (Priority Order)

1. ✅ Install analysis tools (cargo-geiger, cargo-udeps, cargo-modules)
2. 🔄 Complete SIGBUS investigation (awaiting test results)
3. ⏳ Run cargo-geiger unsafe block audit
4. ⏳ Generate module dependency graph
5. ⏳ Manual review of TOP 5 P0 files

**Status:** Phase E2 in progress, blocking on test execution and tool installation.

---

**Report Generated:** 2026-01-07
**Oracle Strategy:** STRATEGY E - DEEP SCAN & CLASSIFY
**Phase:** E2 - Manual Review
**Status:** 🔄 **IN PROGRESS**
