# CLAUDE.md - AI Assistant Guide for Lattice

## 🟦 ARCHITECTURAL RULE: Repository Barrier (SSOT)

**The feature's Repository is the absolute Single Source of Truth for that feature's state. No exceptions.**

This rule exists because we hit a "split brain" bug cluster (April 2026) where models had four parallel sources of truth (SQL, Rust entity, filesystem walks, frontend Zustand). One bug per source-of-truth, all in the same week.

### Rules

1. **No `use_case` or `domain` code may import `std::fs` / `tokio::fs`** to make state decisions. Filesystem walks for "do we have X?" are forbidden — ask the repository instead.
   - ✅ `repository.has_any_embedding_model().await?`
   - ❌ `tokio::fs::read_dir(models_path).await?` to check if a model exists

2. **No use case may issue raw SQL.** Use the repository's typed methods. If the repository lacks the method you need, add it there — don't reach around it.

3. **Zustand is for pure UI preferences ONLY.** Anything the backend reads — settings, indexing config, downloaded model state, conversation drafts, etc. — must use React Query (`useQuery` for reads, `useMutation` for writes) against the Rust repository. The repository is the SSOT; React Query is the read-only mirror. Zustand is reserved exclusively for client-only UI prefs that the backend genuinely doesn't need to know about: theme, sort order, last-active tab, panel collapse state, etc.

   ❌ Don't write a Zustand store that "mirrors" backend state with setter actions and manual rollback — that's paper compliance with the SSOT rule. The store will drift.

   ❌ Don't fall back to `localStorage` for state the backend acts on, even with optimistic-update setters and "we'll save on blur" intentions.

   ✅ Read with `useQuery`, write with `useMutation`. On mutation success, React Query invalidates the cache and refetches the canonical state — no rollback ceremony, no drift.

   See `hooks/queries/useSettingsQuery.ts` and `hooks/queries/useConfigQuery.ts` for the canonical pattern.

4. **No two tables/structs/types may describe the same conceptual entity.** When tempted to add `custom_models` alongside `models`, or `LLMSettings` in TS that doesn't match `LLMSettingsDto` in Rust — STOP. Unify or generate.

5. **TS types for backend DTOs should be generated, not hand-written.** When manually maintained, they drift. (Codegen via `ts-rs` or `specta` — tracked as a follow-up.)

### Tauri command checklist (3 spots, easy to miss)

When adding a new `#[tauri::command]`, ALL THREE must be updated or the frontend gets `Command not found`:

1. **`#[tauri::command]` impl** — in `features/<slice>/plugin/commands.rs`
2. **`tauri::generate_handler![]` registration** — in `features/<slice>/plugin/mod.rs`
3. **`tauri_build::InlinedPlugin::commands(&[...])`** — in `src/build.rs` (this is the one that gets forgotten — it generates the ACL permissions)
4. **`capabilities/main.json`** — add `"<slice>:allow-<command-with-dashes>"` to the permission list

If you skip step 3, the per-command `_<command>.toml` permission file isn't generated, and step 4 fails the build with "Permission not found".

### Enforcement

Run before any PR touching `features/*/use_cases/`:

```bash
bash scripts/check-repository-barrier.sh
```

This grep-based guard fails when a `use_cases/*.rs` file calls `*::read_dir` without an inline justification comment. Legitimate filesystem use (cleaning up artifacts, walking user-provided ingest paths) requires:

```rust
// repository-barrier-allow: <one-sentence reason>
let entries = tokio::fs::read_dir(path).await?;
```

If you find yourself adding the allow comment with a reason like "checking if X exists" — STOP. That's the bug we're trying to prevent. Add a repository method instead.

### When you suspect split-brain

Run the audit: `oracle ask "audit <feature> for split-brain"` with the relevant files. The pattern: same conceptual entity described in ≥2 places that don't sync.

### Examples in this codebase

- ✅ `DownloadedModelRepository` is the SSOT for model state. `first_run_setup.rs` queries it (Phase 3 fix).
- ❌ (fixed) `first_run_setup.rs` used to walk `~/.cache/lattice/models/` non-recursively, missed all subdirectory models.
- ❌ (deleted Phase 5) `custom_models` table was a parallel registry to `models` — orphaned dead code, removed.
- ✅ (Phase 4b) `useSettingsStore` shrunk to display-only. `IndexingTab` + `PrivacyTab` now use React Query against the unified `SettingsRepository`. Privacy flags moved into Rust with a `privacy_gate` helper that future telemetry/crash code MUST consult. Dead toggles (`useQuantization`, `enableAgenticRAG`) deleted — they were decorative, gated nothing.
- ✅ (Task 7) `AppConfig`/`ConfigService` deleted entirely. The 5 fields it owned were either dead (indexed_paths, exclude_patterns, auto_index — no Rust consumers) or duplicated in `LLMSettingsDto` (ollama_endpoint, ollama_model). One-shot migration in `SettingsRepository::new()` ports any leftover `config.json` into `settings.json` at startup, then deletes the legacy file. SSRF validation + path traversal protection moved to `UpdateSettingsUseCase` so a hostile frontend can't bypass via raw `update_settings({category: 'indexing', updates: {indexedPaths: ['/etc/shadow']}})`. Watch folder commands kept as thin wrappers that round-trip through the use case for serialized writes + validation in one place.

---

## AI Assistant Guidelines

### **🔴 THE SACRED RULES - MUST FOLLOW ALWAYS 🔴**

**THESE RULES ARE MANDATORY FOR EVERY TASK - NO EXCEPTIONS**

When implementing ANY feature or change, you MUST follow this workflow:

#### **1. ANALYZE** 🔍
- **Use `zen-architect` agent in ANALYZE mode** to break down the problem
- Understand requirements, constraints, and architecture
- Create a detailed implementation plan
- Identify all affected components

#### **2. ARCHITECT** 🏗️
- **Use `zen-architect` agent in ARCHITECT mode** for system design
- Design the solution following DDD, SOLID, and project patterns
- Specify interfaces, data models, and interactions
- Create specifications for modular-builder

#### **3. IMPLEMENT** ⚙️
- **Use `modular-builder` agent** to implement the specifications
- Follow the architecture and specifications exactly
- Never skip steps or deviate from the plan
- Complete ALL tasks identified in ANALYZE phase

#### **4. VERIFY** ✅
- **Use `zen-architect` agent in REVIEW mode** to verify the implementation
- Ensure code quality, security, and compliance
- Check all requirements are met
- Verify tests pass and coverage is adequate
- **VERIFY BUILD SUCCEEDS** - Run `cargo build` (Rust) or `npm run build` (TypeScript) or `poetry run pytest` (Python)
- Check for compilation errors, type errors, and test failures

#### **5. FIX (if needed)** 🔧
- If verification fails, use appropriate agents to fix issues
- Return to step 4 (VERIFY) after fixes
- Repeat until verification passes

#### **6. NEVER COMMIT** 📝
- Only commit once given express permission to
- Use conventional commit messages
- Include all related changes

**WARNING**:
- ❌ NEVER skip verification - it catches critical issues
- ❌ NEVER implement without analysis and architecture first
- ❌ NEVER commit without verification passing
- ❌ Always use the specified agents - don't try to do it yourself

**Example Workflow**:
```
User: "Add download tracking for models"

1. ANALYZE: zen-architect analyzes requirements
2. ARCHITECT: zen-architect designs the solution
3. IMPLEMENT: modular-builder builds it
4. VERIFY: zen-architect reviews implementation
   - If issues found: fix with bug-hunter/modular-builder, then re-verify
   - If verified: proceed to commit
5. COMMIT: Create commit with changes
```

**Remember**: These rules protect code quality, security, and maintainability. Following them saves time and prevents bugs.

---

<!-- ORACLE-RECOVERY -->
## Oracle Recovery

If context was compacted mid-task, check `FULLAUTO_CONTEXT.md` for task state and run `/fullauto` to resume.
