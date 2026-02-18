# Comprehensive Test Coverage Analysis Report
**Generated**: 2026-01-27
**Project**: Recall Desktop (Vault)
**Analysis Scope**: Complete Rust backend + TypeScript/React frontend

---

## Executive Summary

### Coverage Metrics

#### Rust Backend
- **Total Test Files**: ~68 active test files
- **Disabled Tests**: 13 test files (`.rs.disabled`)
- **Test Modules**: 357 `#[cfg(test)]` modules found
- **Coverage Estimate**: ~65-70% (based on file analysis)

#### TypeScript/React Frontend
- **Total Component Files**: 290 component/hook files
- **Test Files**: 41 test files
- **Test Cases**: 1,102+ individual test cases
- **Coverage Estimate**: ~14% component coverage (41/290 files)
- **Integration Tests**: 4 flow integration tests

### Quality Assessment

**Overall Grade: C+ (Needs Improvement)**

**Strengths:**
- ✅ Strong domain layer test coverage (31 of 31 domain files have tests)
- ✅ Good security test coverage (input validation, rate limiting, audit logging)
- ✅ Comprehensive integration tests for critical paths (indexing, search, tags)
- ✅ Well-structured test helpers and mocks

**Critical Gaps:**
- ❌ **ZERO** tests for new plugin infrastructure (16 plugins, 0 tests)
- ❌ **ZERO** tests for Model Catalog UI components (critical feature)
- ❌ Only 3 of ~24 custom hooks have tests (12.5% coverage)
- ❌ 13 disabled test files indicate test debt
- ❌ Missing tests for error boundary paths
- ❌ No tests for ApiResult wrapper (core IPC pattern)

---

## 1. Critical Gaps (Priority: URGENT)

### 1.1 Plugin Infrastructure (NEW ARCHITECTURE - UNTESTED)

**Location**: `/src/crates/recall/plugins/`
**Criticality**: 🔴 MAXIMUM - New architectural layer with ZERO test coverage

**Untested Plugins** (16 total):
```
❌ plugins/model/commands.rs (13 commands)
❌ plugins/search/commands.rs (6 commands)
❌ plugins/file/commands.rs (12 commands)
❌ plugins/config/commands.rs (5 commands)
❌ plugins/credentials/commands.rs (7 commands)
❌ plugins/health/commands.rs (4 commands)
❌ plugins/tags_plugin.rs
❌ plugins/favorites_plugin.rs
❌ plugins/cache_plugin.rs
❌ plugins/embeddings.rs
❌ plugins/huggingface.rs
❌ plugins/extraction.rs
❌ plugins/conversation_plugin.rs
❌ plugins/batch_plugin.rs
❌ plugins/backup_plugin.rs
❌ plugins/updates_plugin.rs
```

**Why Critical:**
- Plugins are the **primary IPC interface** for frontend-backend communication
- Each plugin handles **91 Result<> returns** across 16 files (error paths untested)
- No validation that plugins correctly delegate to implementation layer
- No tests for tauri-specta type generation
- Coexists with gateway pattern - integration untested

**Required Tests:**

```rust
// File: src/tests/plugins/model_plugin_tests.rs

#[tokio::test]
async fn test_download_model_command_success() {
    // Test: model::download_model delegates correctly
    // Given: Valid download request
    // When: Command invoked via plugin
    // Then: Returns ApiResult::Success and starts download
}

#[tokio::test]
async fn test_download_model_command_invalid_url() {
    // Test: Error handling for invalid URL
    // Given: Malformed URL
    // When: Command invoked
    // Then: Returns ApiResult::Error with VALIDATION_ERROR
}

#[tokio::test]
async fn test_download_model_command_not_found() {
    // Test: 404 handling
    // Given: URL to non-existent model
    // When: Command invoked
    // Then: Returns ApiResult::Error with NOT_FOUND
}

#[tokio::test]
async fn test_download_model_command_network_failure() {
    // Test: Network failure handling
    // Given: Simulated network error
    // When: Command invoked
    // Then: Returns ApiResult::Error with NETWORK_ERROR
}

#[tokio::test]
async fn test_cancel_download_nonexistent_id() {
    // Test: Error handling for missing download
    // Given: Invalid download ID
    // When: Cancel command invoked
    // Then: Returns ApiResult::Error with NOT_FOUND
}

// Repeat pattern for all 13 model commands
```

**Plugin Integration Tests Needed:**

```rust
// File: src/tests/plugins/plugin_integration_test.rs

#[tokio::test]
async fn test_plugin_gateway_coexistence() {
    // Test: Plugins and gateway don't conflict
    // Given: Both plugin and gateway command for same operation
    // When: Both invoked
    // Then: Both return same result
}

#[tokio::test]
async fn test_all_plugins_initialize() {
    // Test: All 16 plugins register successfully
    // Given: Clean Tauri app
    // When: init_plugins() called
    // Then: All plugins registered without errors
}
```

---

### 1.2 ApiResult Wrapper (CORE IPC PATTERN - UNTESTED)

**Location**: `/src/crates/recall/shared/api_result.rs`
**Criticality**: 🔴 MAXIMUM - Used by every IPC command

**Current Status**: ZERO tests for ApiResult wrapper

**Why Critical:**
- ApiResult wraps **every response** from backend to frontend
- Frontend error handling depends on correct serialization
- ErrorCode enum has no validation
- No tests for From<T> conversions (AppError, DomainError, ApplicationError)

**Required Tests:**

```rust
// File: src/crates/recall/shared/api_result.rs (add #[cfg(test)] module)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_success_serialization() {
        // Test: Success variant serializes correctly
        let result = ApiResult::success("test data");
        let json = serde_json::to_string(&result).unwrap();
        assert_eq!(json, r#"{"ok":true,"data":"test data"}"#);
    }

    #[test]
    fn test_error_serialization() {
        // Test: Error variant serializes correctly
        let result = ApiResult::<()>::error(
            ErrorCode::NotFound,
            "Resource not found"
        );
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains(r#""ok":false"#));
        assert!(json.contains(r#""code":"NOT_FOUND""#));
    }

    #[test]
    fn test_error_with_details_serialization() {
        // Test: Error with details includes details field
        let result = ApiResult::<()>::error_with_details(
            ErrorCode::ValidationError,
            "Invalid input",
            "Field 'email' is required"
        );
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains(r#""details":"Field 'email' is required""#));
    }

    #[test]
    fn test_from_app_error() {
        // Test: Conversion from AppError
        let app_error = AppError::NotFound("Document not found".into());
        let result: ApiResult<()> = ApiResult::from(app_error);
        assert!(!result.is_ok());
    }

    #[test]
    fn test_from_domain_error() {
        // Test: Conversion from DomainError
        let domain_error = DomainError::InvalidState("Invalid state".into());
        let result: ApiResult<()> = ApiResult::from(domain_error);
        assert!(!result.is_ok());
    }

    #[test]
    fn test_error_code_mapping() {
        // Test: All ErrorCode variants map correctly
        let codes = vec![
            ErrorCode::NotFound,
            ErrorCode::ValidationError,
            ErrorCode::PermissionDenied,
            ErrorCode::InternalError,
            // ... test all variants
        ];
        for code in codes {
            let result = ApiResult::<()>::error(code.clone(), "test");
            let json = serde_json::to_string(&result).unwrap();
            assert!(json.contains(r#""ok":false"#));
        }
    }
}
```

---

### 1.3 Model Catalog UI Components (USER-FACING - UNTESTED)

**Location**: `/websrc/components/Settings/ModelCatalog/`
**Criticality**: 🔴 HIGH - Core feature with complex state management

**Untested Components** (9 files):
```
❌ ModelCard.tsx (compatibility display logic)
❌ ModelDetailPanel.tsx (model details + download button)
❌ ModelListView.tsx (list rendering + filtering)
❌ ModelSearchBar.tsx (search input + filtering)
❌ ModelFilterPanel.tsx (filter UI logic)
❌ ModelCatalogBrowser.tsx (main component)
❌ CatalogManagementSection.tsx (catalog management)
❌ SystemCapabilitiesCard.tsx (system info display)
```

**Why Critical:**
- User-facing feature for downloading models
- Complex state management (3 hooks: useModelCatalog, useDownloads, useDownloadedModels)
- No tests for download flow (start, progress, cancel, error states)
- No tests for compatibility filtering
- No tests for error boundary integration

**Required Tests:**

```typescript
// File: websrc/components/Settings/ModelCatalog/__tests__/ModelCard.test.tsx

describe('ModelCard', () => {
  it('displays compatibility badge correctly', () => {
    // Test: Compatibility levels render with correct styling
    const compatibilityLevels: CompatibilityLevel[] = [
      'Excellent', 'Good', 'Poor', 'Incompatible'
    ];
    // Render each and verify styles
  });

  it('handles click events', async () => {
    // Test: onClick prop called when card clicked
  });

  it('shows selected state', () => {
    // Test: Selected card has ring styling
  });

  it('displays model metadata', () => {
    // Test: Name, size, tier displayed correctly
  });
});

// File: websrc/components/Settings/ModelCatalog/__tests__/ModelDetailPanel.test.tsx

describe('ModelDetailPanel', () => {
  it('renders model details correctly', () => {
    // Test: All model fields displayed
  });

  it('handles download button click', async () => {
    // Test: Download initiated when button clicked
  });

  it('shows download progress', () => {
    // Test: Progress bar updates during download
  });

  it('handles download errors', async () => {
    // Test: Error message shown on download failure
  });

  it('disables download for incompatible models', () => {
    // Test: Button disabled for incompatible models
  });
});
```

**Hook Integration Tests:**

```typescript
// File: websrc/hooks/__tests__/useModelCatalog.test.tsx

describe('useModelCatalog', () => {
  it('fetches catalog on mount', async () => {
    // Test: Catalog loaded when hook initialized
  });

  it('handles catalog fetch errors', async () => {
    // Test: Error state set on fetch failure
  });

  it('filters models by compatibility', () => {
    // Test: Filter function works correctly
  });

  it('searches models by name', () => {
    // Test: Search function filters list
  });
});
```

---

### 1.4 Custom Hooks (CRITICAL LOGIC - MOSTLY UNTESTED)

**Location**: `/websrc/hooks/`
**Coverage**: 3 of 24 hooks have tests (12.5%)

**Untested Critical Hooks**:
```
❌ useModelCatalog.ts (281 lines) - Model catalog state management
❌ useDownloads.ts (147 lines) - Download state + event listening
❌ useDownloadedModels.ts (301 lines) - Downloaded model management
❌ useOptimizedSearch.ts - Search optimization logic
❌ useDownloadState.ts - Download state machine
❌ useAggregatedDownloads.ts - Download aggregation
❌ useErrorRecovery.ts - Error recovery logic
❌ useCommandPalette.ts - Command palette state
❌ useFileContent.ts - File content loading
❌ usePerformanceMonitor.ts - Performance tracking
❌ useFirstRun.ts - First run detection
❌ useTauriCommand.ts - Generic command wrapper
❌ useDebounce.ts - Debouncing logic
❌ useThrottle.ts - Throttling logic
```

**Why Critical:**
- Hooks contain **business logic** that should be in backend
- Complex state management without verification
- Error handling paths untested
- Event listener cleanup untested (memory leaks)

**Required Tests:**

```typescript
// File: websrc/hooks/__tests__/useDownloads.test.tsx

import { renderHook, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useDownloads } from '../useDownloads';
import { listen } from '@tauri-apps/api/event';

vi.mock('@tauri-apps/api/event');

describe('useDownloads', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('subscribes to download events on mount', () => {
    // Test: Event listener registered
    renderHook(() => useDownloads());
    expect(listen).toHaveBeenCalledWith('download:progress', expect.any(Function));
  });

  it('unsubscribes on unmount', () => {
    // Test: Event listener cleaned up
    const { unmount } = renderHook(() => useDownloads());
    const unsubscribe = vi.fn();
    (listen as any).mockResolvedValue(unsubscribe);
    unmount();
    expect(unsubscribe).toHaveBeenCalled();
  });

  it('updates download state on progress event', async () => {
    // Test: State updates when event received
    const { result } = renderHook(() => useDownloads());
    // Simulate event
    await waitFor(() => {
      expect(result.current.downloads).toBeDefined();
    });
  });

  it('handles download errors', async () => {
    // Test: Error state set on download failure
  });

  it('aggregates multiple downloads correctly', () => {
    // Test: Multiple downloads tracked independently
  });
});
```

---

### 1.5 Download State Machine (COMPLEX LOGIC - PARTIALLY TESTED)

**Location**: `/src/crates/recall/domain/download.rs`
**Current**: Basic state transition tests exist
**Missing**: Edge cases and error scenarios

**Untested Scenarios:**
```
❌ Invalid state transitions (e.g., Completed → Downloading)
❌ Concurrent state updates (race conditions)
❌ State persistence failures
❌ Resume after application restart
❌ Cleanup on cancel (file removal)
❌ Checksum mismatch handling
❌ Network timeout during download
❌ Disk full during download
❌ Partial file corruption detection
```

**Required Tests:**

```rust
// File: src/tests/domain/download_state_machine_tests.rs

#[test]
fn test_invalid_state_transition_rejected() {
    // Test: Invalid transitions return error
    let state = DownloadState::Completed;
    assert!(!state.can_transition_to(&DownloadState::Downloading));
}

#[tokio::test]
async fn test_concurrent_state_updates() {
    // Test: Concurrent updates handled safely
    // Given: Download in progress
    // When: Multiple threads update state
    // Then: Final state is consistent
}

#[tokio::test]
async fn test_resume_after_restart() {
    // Test: Download resumes from checkpoint
    // Given: Download at 50% when app crashes
    // When: App restarts and resume called
    // Then: Download continues from 50%
}

#[tokio::test]
async fn test_disk_full_during_download() {
    // Test: Graceful handling of disk full
    // Given: Download in progress
    // When: Disk full error occurs
    // Then: State transitions to Failed with IOError
}

#[tokio::test]
async fn test_checksum_mismatch() {
    // Test: Invalid checksum detected
    // Given: Downloaded file with wrong checksum
    // When: Validation runs
    // Then: Returns ChecksumMismatch error
}
```

---

### 1.6 Error Boundaries (ERROR PATHS - UNTESTED)

**Location**: `/websrc/components/ErrorBoundary/`
**Current**: 1 basic test for ErrorBoundary.tsx
**Missing**: Error boundary activation scenarios

**Untested Components**:
```
✅ ErrorBoundary.tsx (1 test)
❌ FeatureErrorBoundary.tsx (feature-level boundaries)
❌ SectionErrorBoundary.tsx (section-level boundaries)
❌ InlineError.tsx (inline error display)
❌ SectionError.tsx (section error display)
❌ FullPageError.tsx (full page error display)
```

**Why Critical:**
- Error boundaries are **last line of defense** against crashes
- No tests verify boundaries actually catch errors
- No tests for error recovery actions
- No tests for error logging integration

**Required Tests:**

```typescript
// File: websrc/components/ErrorBoundary/__tests__/FeatureErrorBoundary.test.tsx

describe('FeatureErrorBoundary', () => {
  it('catches render errors in children', () => {
    // Test: Error boundary catches child component errors
    const ThrowError = () => { throw new Error('Test error'); };
    render(
      <FeatureErrorBoundary>
        <ThrowError />
      </FeatureErrorBoundary>
    );
    expect(screen.getByText(/something went wrong/i)).toBeInTheDocument();
  });

  it('shows fallback UI on error', () => {
    // Test: Custom fallback displayed
  });

  it('allows error recovery', async () => {
    // Test: Reset button restores component
  });

  it('logs errors to backend', () => {
    // Test: Error logged via IPC
  });
});
```

---

## 2. Module-by-Module Analysis

### 2.1 Rust Backend

#### Application Services
**Location**: `/src/crates/recall/application/services/`

| Module | Tests | Status | Priority |
|--------|-------|--------|----------|
| model_service.rs | ✅ Yes | Partial | Medium |
| model_reconciliation_service.rs | ✅ Yes | Partial | Medium |
| file_type_detector.rs | ✅ Yes | Good | Low |
| conversation_summarizer.rs | ❌ No | **Missing** | High |
| context_window_builder.rs | ✅ Yes | Good | Low |

**Critical Gap**: conversation_summarizer.rs has NO tests

```rust
// File: src/tests/application/conversation_summarizer_tests.rs

#[tokio::test]
async fn test_summarize_conversation_basic() {
    // Test: Basic conversation summarization
    // Given: Conversation with 10 messages
    // When: summarize() called
    // Then: Returns concise summary
}

#[tokio::test]
async fn test_summarize_empty_conversation() {
    // Test: Empty input handling
    // Given: Empty conversation
    // When: summarize() called
    // Then: Returns appropriate error or empty result
}

#[tokio::test]
async fn test_summarize_very_long_conversation() {
    // Test: Context window overflow handling
    // Given: Conversation exceeding context window
    // When: summarize() called
    // Then: Chunks conversation and summarizes parts
}
```

#### Domain Layer
**Status**: ✅ **EXCELLENT** - 31 of 31 domain files have tests

#### Infrastructure Layer
**Location**: `/src/crates/recall/infrastructure/`

| Module | Tests | Status | Priority |
|--------|-------|--------|----------|
| download_manager.rs | ✅ Yes | Good | Low |
| download_engine.rs | ❌ Disabled | **Needs Re-enable** | High |
| search/* | ✅ Yes | Good | Low |
| indexing/* | ✅ Yes | Good | Low |
| ml/* | ✅ Yes | Partial | Medium |
| web/* | ✅ Yes | Good | Low |
| security/* | ✅ Yes | Excellent | Low |

**Critical**: download_engine_tests.rs.disabled needs investigation

---

### 2.2 TypeScript/React Frontend

#### Components Coverage Summary

| Category | Total | Tested | Coverage | Priority |
|----------|-------|--------|----------|----------|
| **Settings** | 15 | 4 | 27% | High |
| **Model Catalog** | 9 | 0 | **0%** | 🔴 URGENT |
| **Search** | 8 | 2 | 25% | High |
| **Chat** | 9 | 2 | 22% | High |
| **File Browser** | 6 | 3 | 50% | Medium |
| **Error Boundaries** | 6 | 1 | 17% | High |
| **Progress** | 6 | 1 | 17% | High |
| **UI Components** | ~40 | 5 | 13% | Low |

#### Hooks Coverage

| Hook Category | Total | Tested | Coverage |
|---------------|-------|--------|----------|
| Data Fetching | 8 | 2 | 25% |
| State Management | 6 | 1 | 17% |
| Side Effects | 5 | 0 | **0%** |
| Utilities | 5 | 0 | **0%** |

---

## 3. Edge Case Gaps

### 3.1 Race Conditions

**Status**: ❌ Race condition tests disabled

```bash
tests/tag_race_condition_test.rs.disabled
tests/parallel_execution_test.rs.disabled
```

**Critical Scenarios Missing:**
```rust
// Concurrent tag updates
#[tokio::test]
async fn test_concurrent_tag_creation() {
    // Test: Multiple threads creating same tag
    // Given: 10 threads creating tag "Important"
    // When: All execute simultaneously
    // Then: Only one tag created, others get existing ID
}

// Concurrent download updates
#[tokio::test]
async fn test_concurrent_download_progress_updates() {
    // Test: Progress updates from multiple chunks
    // Given: Multi-threaded download
    // When: Multiple threads report progress
    // Then: Progress aggregates correctly
}

// Concurrent search requests
#[tokio::test]
async fn test_concurrent_search_requests() {
    // Test: Multiple searches don't interfere
    // Given: 5 simultaneous search queries
    // When: All execute
    // Then: Each returns correct results
}
```

### 3.2 Boundary Conditions

**Empty Inputs:**
```
❌ Empty search query handling
❌ Empty file upload handling
❌ Empty tag name handling
❌ Zero-byte file indexing
```

**Maximum Limits:**
```
❌ Maximum file size handling
❌ Maximum search results handling
❌ Maximum tag count per document
❌ Maximum conversation length
```

**Off-by-One:**
```
❌ Pagination edge cases (page 0, negative page)
❌ Chunk boundary handling in search
❌ Vector index size limits
```

**Required Tests:**

```rust
// File: src/tests/boundary_conditions/search_boundaries.rs

#[tokio::test]
async fn test_search_empty_query() {
    // Test: Empty query handling
    let result = search_service.search("", 10).await;
    assert!(result.is_err() || result.unwrap().is_empty());
}

#[tokio::test]
async fn test_search_maximum_results() {
    // Test: Requesting more than available
    let result = search_service.search("test", usize::MAX).await;
    assert!(result.is_ok());
    assert!(result.unwrap().len() <= MAX_SEARCH_RESULTS);
}

#[tokio::test]
async fn test_index_zero_byte_file() {
    // Test: Empty file handling
    let result = indexing_service.index_file(empty_file_path).await;
    assert!(result.is_ok());
}
```

### 3.3 Network Failures

**Status**: Basic tests exist, missing comprehensive failure scenarios

**Missing Scenarios:**
```
❌ Partial response handling (connection drops mid-transfer)
❌ DNS resolution failures
❌ SSL certificate validation errors
❌ Redirect loops
❌ Slow response timeouts
❌ Server returns 429 (rate limiting)
❌ Server returns 503 (service unavailable)
❌ Chunked transfer encoding errors
```

**Required Tests:**

```rust
// File: src/tests/network/failure_scenarios.rs

#[tokio::test]
async fn test_download_partial_response() {
    // Test: Connection drops mid-transfer
    // Given: Download at 50%
    // When: Connection lost
    // Then: State transitions to Failed, retry possible
}

#[tokio::test]
async fn test_download_redirect_loop() {
    // Test: Infinite redirect detection
    // Given: URL that redirects to itself
    // When: Download attempted
    // Then: Returns error after max redirects
}

#[tokio::test]
async fn test_download_rate_limited() {
    // Test: 429 response handling
    // Given: Server returns 429
    // When: Download attempted
    // Then: Respects Retry-After header
}
```

### 3.4 Disk Space Failures

**Status**: ❌ No disk space failure tests

**Required Tests:**

```rust
#[tokio::test]
async fn test_indexing_disk_full() {
    // Test: Disk full during indexing
    // Given: Vector store write
    // When: Disk full
    // Then: Graceful error, no corruption
}

#[tokio::test]
async fn test_download_disk_full() {
    // Test: Disk full during download
    // Given: Download in progress
    // When: Disk full
    // Then: State saved, cleanup triggered
}
```

---

## 4. Test Quality Issues

### 4.1 Disabled Tests (Technical Debt)

**Status**: 13 test files disabled

```bash
❌ download_engine_tests.rs.disabled
❌ download_integration_test.rs.disabled
❌ function_registry_tests.rs.disabled
❌ llm_performance_benchmarks.rs.disabled
❌ parallel_execution_test.rs.disabled
❌ patterns_integration_test.rs.disabled
❌ phase1_integration.rs.disabled
❌ query_classifier_test.rs.disabled
❌ service_integration_tests.rs.disabled
❌ tag_race_condition_test.rs.disabled
❌ test_embedding_dimensions.rs.disabled
❌ test_indexing_performance_benchmarks.rs.disabled
❌ web_service_tests.rs.disabled
```

**Action Required**: Investigation + re-enablement plan

### 4.2 Mock Quality

**Status**: ✅ Good mock infrastructure exists

**Location**: `/src/tests/mocks/`

**Strengths:**
- MockDownloadRepository
- MockDownloadEngine
- MockTauriEventEmitter (TypeScript)

**Gaps:**
- No mock for LLM service (conversation tests)
- No mock for vector search service
- No mock for file system operations

### 4.3 Integration Test Coverage

**Status**: ✅ Good coverage for existing features

**Existing Integration Tests:**
```
✅ tag_generator_integration_test.rs
✅ test_search_integration.rs
✅ link_parser_integration_test.rs
✅ test_end_to_end_indexing.rs
✅ ipc_commands_test.rs
✅ test_tag_integration.rs
✅ hybrid_search_integration_test.rs
```

**Missing Integration Tests:**
```
❌ Plugin → Implementation integration
❌ Model download → Vector store integration
❌ Frontend → Backend E2E for Model Catalog
❌ Error propagation through layers
```

### 4.4 Test Flakiness

**Potential Issues:**
- Time-dependent tests (sleep statements in download tests)
- Filesystem operations without proper cleanup
- Event listener tests without proper unsubscribe

**Recommendations:**
```rust
// Bad: Time-dependent
sleep(Duration::from_millis(100)).await;

// Good: Wait for condition
wait_for_condition(|| download.is_complete(), Duration::from_secs(5)).await;
```

---

## 5. Recommendations (Prioritized)

### Phase 1: Critical Infrastructure (Week 1-2)

**Priority: 🔴 URGENT**

1. **Plugin Infrastructure Tests** (3-5 days)
   - Create test file for each of 16 plugins
   - Test all 91 Result<> paths
   - Test plugin-gateway coexistence
   - **Effort**: 40-60 tests

2. **ApiResult Wrapper Tests** (1 day)
   - Test serialization (success/error)
   - Test all ErrorCode mappings
   - Test From<> conversions
   - **Effort**: 15-20 tests

3. **Model Catalog UI Tests** (2-3 days)
   - Test all 9 components
   - Test 3 critical hooks
   - Test download flow integration
   - **Effort**: 50-70 tests

### Phase 2: Critical Hooks (Week 2-3)

**Priority: 🟡 HIGH**

1. **useDownloads Hook** (1 day)
   - Event subscription/cleanup
   - State updates
   - Error handling
   - **Effort**: 15-20 tests

2. **useModelCatalog Hook** (1 day)
   - Data fetching
   - Filtering/searching
   - Error states
   - **Effort**: 15-20 tests

3. **useDownloadedModels Hook** (1 day)
   - Model listing
   - Active model management
   - Deletion handling
   - **Effort**: 15-20 tests

### Phase 3: Edge Cases (Week 3-4)

**Priority: 🟢 MEDIUM**

1. **Race Condition Tests** (2 days)
   - Re-enable disabled tests
   - Add concurrent operation tests
   - **Effort**: 20-30 tests

2. **Boundary Condition Tests** (1 day)
   - Empty inputs
   - Maximum limits
   - Off-by-one scenarios
   - **Effort**: 20-30 tests

3. **Network Failure Tests** (1 day)
   - Comprehensive HTTP error scenarios
   - Timeout handling
   - Retry logic
   - **Effort**: 15-20 tests

### Phase 4: Error Boundaries (Week 4)

**Priority: 🟢 MEDIUM**

1. **Error Boundary Tests** (1 day)
   - Test all 6 boundary components
   - Test error recovery
   - Test error logging
   - **Effort**: 20-30 tests

### Phase 5: Re-enable Disabled Tests (Ongoing)

**Priority: 🟡 HIGH**

1. **Investigate Disabled Tests** (3 days)
   - Determine why each was disabled
   - Fix or remove
   - Document decisions
   - **Effort**: Investigation + fixes

---

## 6. Specific Test Cases

### 6.1 Plugin Command Tests (Template)

```rust
// File: src/tests/plugins/template_plugin_tests.rs

use crate::plugins::{model, search, file}; // etc.
use crate::shared::api_result::{ApiResult, ErrorCode};

// Pattern 1: Success case
#[tokio::test]
async fn test_[command]_success() {
    // Arrange: Setup mocks/test data
    let app_state = setup_test_app_state().await;

    // Act: Invoke command
    let result = [plugin]::[command](app_state, valid_request).await;

    // Assert: Success
    assert!(matches!(result, ApiResult::Success { .. }));
}

// Pattern 2: Validation error
#[tokio::test]
async fn test_[command]_validation_error() {
    // Arrange
    let app_state = setup_test_app_state().await;

    // Act: Invalid input
    let result = [plugin]::[command](app_state, invalid_request).await;

    // Assert: Validation error
    assert!(matches!(result, ApiResult::Error {
        error: ApiError { code: ErrorCode::ValidationError, .. }
    }));
}

// Pattern 3: Not found error
#[tokio::test]
async fn test_[command]_not_found() {
    // Arrange
    let app_state = setup_test_app_state().await;

    // Act: Non-existent resource
    let result = [plugin]::[command](app_state, nonexistent_id).await;

    // Assert: Not found
    assert!(matches!(result, ApiResult::Error {
        error: ApiError { code: ErrorCode::NotFound, .. }
    }));
}

// Pattern 4: Permission denied
#[tokio::test]
async fn test_[command]_permission_denied() {
    // Arrange
    let app_state = setup_test_app_state().await;

    // Act: Unauthorized request
    let result = [plugin]::[command](app_state, unauthorized_request).await;

    // Assert: Permission denied
    assert!(matches!(result, ApiResult::Error {
        error: ApiError { code: ErrorCode::PermissionDenied, .. }
    }));
}

// Pattern 5: Internal error
#[tokio::test]
async fn test_[command]_internal_error() {
    // Arrange: Inject failure
    let app_state = setup_test_app_state_with_failing_service().await;

    // Act
    let result = [plugin]::[command](app_state, valid_request).await;

    // Assert: Internal error
    assert!(matches!(result, ApiResult::Error {
        error: ApiError { code: ErrorCode::InternalError, .. }
    }));
}
```

**Apply this template to ALL 47 plugin commands**

### 6.2 Hook Tests (Template)

```typescript
// File: websrc/hooks/__tests__/[hookName].test.tsx

import { renderHook, waitFor, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { [hookName] } from '../[hookName]';

describe('[hookName]', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  // Pattern 1: Initial state
  it('initializes with correct default state', () => {
    const { result } = renderHook(() => [hookName]());
    expect(result.current.loading).toBe(true);
    expect(result.current.data).toBeNull();
    expect(result.current.error).toBeNull();
  });

  // Pattern 2: Success case
  it('fetches data successfully', async () => {
    const { result } = renderHook(() => [hookName]());
    await waitFor(() => {
      expect(result.current.loading).toBe(false);
      expect(result.current.data).toBeDefined();
    });
  });

  // Pattern 3: Error handling
  it('handles fetch errors', async () => {
    // Mock error
    vi.mocked(invoke).mockRejectedValue(new Error('API Error'));

    const { result } = renderHook(() => [hookName]());
    await waitFor(() => {
      expect(result.current.error).toBeDefined();
    });
  });

  // Pattern 4: Cleanup
  it('cleans up resources on unmount', () => {
    const unsubscribe = vi.fn();
    // Setup subscription mock
    const { unmount } = renderHook(() => [hookName]());
    unmount();
    expect(unsubscribe).toHaveBeenCalled();
  });

  // Pattern 5: Action handling
  it('handles user actions correctly', async () => {
    const { result } = renderHook(() => [hookName]());
    await act(async () => {
      await result.current.someAction();
    });
    expect(result.current.data).toMatchExpectedState();
  });
});
```

### 6.3 Component Tests (Template)

```typescript
// File: websrc/components/[Component]/__tests__/[Component].test.tsx

import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi } from 'vitest';
import { [Component] } from '../[Component]';

describe('[Component]', () => {
  const defaultProps = {
    // Define required props
  };

  // Pattern 1: Rendering
  it('renders correctly', () => {
    render(<[Component] {...defaultProps} />);
    expect(screen.getByRole('...')).toBeInTheDocument();
  });

  // Pattern 2: User interaction
  it('handles user interactions', async () => {
    const user = userEvent.setup();
    const onAction = vi.fn();

    render(<[Component] {...defaultProps} onAction={onAction} />);
    await user.click(screen.getByRole('button'));
    expect(onAction).toHaveBeenCalled();
  });

  // Pattern 3: State changes
  it('updates UI on state changes', async () => {
    const { rerender } = render(<[Component] {...defaultProps} />);
    rerender(<[Component] {...defaultProps} data={newData} />);
    expect(screen.getByText(/new data/i)).toBeInTheDocument();
  });

  // Pattern 4: Error states
  it('displays error states', () => {
    render(<[Component] {...defaultProps} error="Error message" />);
    expect(screen.getByText(/error message/i)).toBeInTheDocument();
  });

  // Pattern 5: Loading states
  it('displays loading states', () => {
    render(<[Component] {...defaultProps} loading={true} />);
    expect(screen.getByRole('progressbar')).toBeInTheDocument();
  });
});
```

---

## 7. Test Metrics Targets

### Immediate Goals (1 Month)

| Metric | Current | Target | Status |
|--------|---------|--------|--------|
| **Rust Coverage** | 65% | 75% | 🟡 Achievable |
| **TypeScript Coverage** | 14% | 40% | 🟡 Achievable |
| **Plugin Tests** | 0% | 100% | 🔴 Critical |
| **Hook Tests** | 12.5% | 60% | 🟡 Achievable |
| **Integration Tests** | Good | Excellent | ✅ On track |
| **Disabled Tests** | 13 | 0 | 🔴 Requires effort |

### Long-term Goals (3 Months)

| Metric | Target | Notes |
|--------|--------|-------|
| **Rust Coverage** | 85% | Focus on error paths |
| **TypeScript Coverage** | 70% | Focus on components/hooks |
| **E2E Tests** | 10 flows | Critical user journeys |
| **Performance Tests** | Re-enabled | Benchmarks for search/indexing |
| **Race Condition Tests** | Re-enabled | Concurrent operation safety |

---

## 8. Testing Infrastructure Recommendations

### 8.1 Current Infrastructure

**Strengths:**
- ✅ Vitest setup for TypeScript
- ✅ Good mock infrastructure (MockTauriEventEmitter, etc.)
- ✅ Integration test harness exists
- ✅ Test helpers in common module

**Gaps:**
- ❌ No test coverage reporting
- ❌ No CI test enforcement
- ❌ No mutation testing
- ❌ No snapshot testing for UI

### 8.2 Recommended Additions

1. **Coverage Reporting**
   ```bash
   # Rust
   cargo install cargo-tarpaulin
   cargo tarpaulin --out Html

   # TypeScript
   npm run test:coverage
   ```

2. **CI Integration**
   ```yaml
   # .github/workflows/test.yml
   - name: Run Tests
     run: |
       cargo test --all-features
       npm run test:ci
   - name: Check Coverage
     run: |
       cargo tarpaulin --fail-under 75
   ```

3. **Snapshot Testing**
   ```typescript
   // For UI components
   it('matches snapshot', () => {
     const { container } = render(<ModelCard {...props} />);
     expect(container).toMatchSnapshot();
   });
   ```

4. **Property-Based Testing**
   ```rust
   // For domain logic
   use proptest::prelude::*;

   proptest! {
       #[test]
       fn test_download_state_machine(
           transitions in prop::collection::vec(any::<StateTransition>(), 1..100)
       ) {
           // Test arbitrary state transition sequences
       }
   }
   ```

---

## 9. Red Flags / Anti-Patterns Detected

### 9.1 Test Code Smells

1. **Excessive Sleep Statements**
   - Location: download_manager_tests.rs
   - Issue: `sleep(Duration::from_millis(100))` instead of waiting for conditions
   - Fix: Use condition-based waiting

2. **Disabled Tests Without Explanation**
   - Location: 13 .rs.disabled files
   - Issue: No comments explaining why disabled
   - Fix: Add `// DISABLED: [reason]` comments

3. **Missing Cleanup in Tests**
   - Location: Various integration tests
   - Issue: Test files/directories not removed
   - Fix: Use tempfile crate consistently

### 9.2 Production Code Smells

1. **Business Logic in Hooks**
   - Location: useDownloads.ts, useModelCatalog.ts
   - Issue: Complex logic in frontend instead of backend
   - Fix: Move to backend services, hooks should be thin

2. **Error Swallowing**
   - Location: Various .catch(() => {}) without logging
   - Issue: Silent failures hide bugs
   - Fix: Always log errors to backend

3. **No Input Validation in Plugins**
   - Location: All plugin commands
   - Issue: Plugins trust input from frontend
   - Fix: Add validation layer in plugins

---

## 10. Action Plan Summary

### Immediate Actions (This Week)

1. ✅ **Create test file for model plugin** (highest priority)
   - File: `src/tests/plugins/model_plugin_tests.rs`
   - Tests: All 13 commands × 5 scenarios = 65 tests

2. ✅ **Create ApiResult tests**
   - File: `src/crates/recall/shared/api_result.rs` (add test module)
   - Tests: 15-20 tests

3. ✅ **Create ModelCard tests**
   - File: `websrc/components/Settings/ModelCatalog/__tests__/ModelCard.test.tsx`
   - Tests: 10-15 tests

### Short-term Actions (Next 2 Weeks)

1. Complete plugin test coverage (all 16 plugins)
2. Test all 3 critical model management hooks
3. Test Model Catalog UI components
4. Re-enable download_engine_tests.rs

### Medium-term Actions (Next Month)

1. Achieve 75% Rust coverage
2. Achieve 40% TypeScript coverage
3. Add race condition tests
4. Add boundary condition tests
5. Fix all disabled tests

### Long-term Actions (Next Quarter)

1. Achieve 85% Rust coverage
2. Achieve 70% TypeScript coverage
3. Add E2E tests for critical flows
4. Implement mutation testing
5. Add performance regression tests

---

## Conclusion

**Current State**: The codebase has **good foundational test coverage** for established features (domain layer, search, indexing) but **critical gaps in new architecture** (plugins, Model Catalog UI).

**Biggest Risks**:
1. 🔴 **Plugin infrastructure (47 commands, 0 tests)** - New IPC layer completely untested
2. 🔴 **ApiResult wrapper (0 tests)** - Core error handling pattern unverified
3. 🔴 **Model Catalog UI (9 components, 0 tests)** - User-facing feature untested

**Recommended Focus**: Prioritize plugin tests (Phase 1) as they represent the **greatest architectural risk** and newest code without coverage.

**Effort Estimate**:
- Phase 1 (Critical): 5-7 days (plugin + ApiResult + UI tests)
- Phase 2 (Hooks): 3-4 days
- Phase 3 (Edge Cases): 4-5 days
- Phase 4 (Error Boundaries): 1-2 days
- **Total**: 3-4 weeks for comprehensive coverage improvement

**Success Metrics**:
- ✅ Plugin coverage: 0% → 100%
- ✅ TypeScript coverage: 14% → 40%
- ✅ Rust coverage: 65% → 75%
- ✅ Disabled tests: 13 → 0
- ✅ Critical hooks: 12.5% → 60%

---

**End of Report**
