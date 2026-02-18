# Tauri Desktop Application - Developer Guide

**Version**: 1.0
**Last Updated**: 2025-01-17
**Audience**: Developers working on the Recall desktop application

This guide provides step-by-step instructions for common development tasks. For architectural concepts, see [ARCHITECTURE.md](ARCHITECTURE.md).

---

## Table of Contents

- [Getting Started](#getting-started)
- [Adding a New Command](#adding-a-new-command)
- [Adding a New Service](#adding-a-new-service)
- [Adding a New Domain Model](#adding-a-new-domain-model)
- [Adding Security Controls](#adding-security-controls)
- [Testing Guide](#testing-guide)
- [Debugging Guide](#debugging-guide)
- [Common Patterns](#common-patterns)
- [Troubleshooting](#troubleshooting)

---

## Getting Started

### Prerequisites

- Rust (latest stable) - `rustup update stable`
- Node.js 18+ and npm
- SQLite 3.35+
- Git

### Initial Setup

```bash
# Clone the repository
git clone https://github.com/yourusername/Recall.git
cd Recall/vault/desktop

# Install frontend dependencies
npm install

# Build Rust backend
cd src-tauri
cargo build

# Run development server
cd ..
npm run tauri:dev
```

### Project Structure

```
vault/desktop/
├── src/                          # React frontend
│   ├── components/              # UI components
│   ├── stores/                  # Zustand state management
│   └── main.tsx                 # App entry point
├── src-tauri/                   # Rust backend
│   ├── src/
│   │   ├── commands/           # Tauri command handlers (IPC layer)
│   │   ├── services/           # Business logic services
│   │   ├── domain/             # Domain models (DDD)
│   │   ├── repositories/       # Data access layer
│   │   ├── security/           # Security controls
│   │   ├── audit/              # Audit logging
│   │   ├── di/                 # Dependency injection
│   │   └── main.rs             # Rust entry point
│   ├── tests/                  # Test suite
│   └── Cargo.toml              # Rust dependencies
└── package.json                # Frontend dependencies
```

---

## Adding a New Command

Commands are the IPC layer between the frontend and backend. They should be **thin controllers** that delegate to services.

### Step 1: Create Command File

Create a new file in `src-tauri/src/commands/`:

```bash
cd src-tauri/src/commands
touch my_feature.rs
```

### Step 2: Define Command Structure

**File**: `src-tauri/src/commands/my_feature.rs`

```rust
use crate::di::ServiceContainer;
use crate::error::{AppError, Result};
use crate::audit::{audit_success, audit_failure, AuditAction};
use serde::{Deserialize, Serialize};
use tauri::State;

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MyFeatureRequest {
    pub input: String,
    pub options: Option<MyFeatureOptions>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MyFeatureOptions {
    pub limit: Option<usize>,
    pub filter: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MyFeatureResponse {
    pub result: String,
    pub count: usize,
    pub execution_time_ms: u64,
}

// ============================================================================
// Command Handler
// ============================================================================

/// Performs my feature operation
///
/// # Security
/// - Rate limited to 100 requests/minute
/// - Input validated for length and content
/// - Operation is audit logged
///
/// # Example
/// ```typescript
/// const response = await invoke<MyFeatureResponse>('my_feature_command', {
///   request: {
///     input: 'test input',
///     options: { limit: 10 }
///   }
/// });
/// ```
#[tauri::command]
pub async fn my_feature_command(
    container: State<'_, ServiceContainer>,
    request: MyFeatureRequest,
) -> Result<MyFeatureResponse> {
    let start = std::time::Instant::now();

    // 1. Rate limiting (prevent DoS)
    container
        .security_context()
        .rate_limiters
        .my_operation  // Define this in security/rate_limiter.rs
        .check()
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    // 2. Input validation
    let validated_input = container
        .security_context()
        .input_validator
        .validate_input(&request.input)?;

    // 3. Get services from container
    let my_service = container.my_service();

    // 4. Execute business logic (delegate to service)
    let result = match my_service.do_something(&validated_input).await {
        Ok(res) => {
            // Success: audit log
            audit_success!(
                action = AuditAction::MyFeaturePerformed,
                resource_type = "my_feature",
                resource_id = validated_input.as_str(),
                metadata = ("result_count", res.count.to_string().as_str())
            );
            res
        }
        Err(e) => {
            // Failure: audit log with error
            audit_failure!(
                action = AuditAction::MyFeaturePerformed,
                resource_type = "my_feature",
                resource_id = validated_input.as_str(),
                error = e.to_string()
            );
            return Err(e);
        }
    };

    // 5. Build response
    let execution_time_ms = start.elapsed().as_millis() as u64;

    Ok(MyFeatureResponse {
        result: result.data,
        count: result.count,
        execution_time_ms,
    })
}
```

### Step 3: Register Command

**File**: `src-tauri/src/commands/mod.rs`

Add your module:
```rust
pub mod my_feature;

pub use my_feature::my_feature_command;
```

**File**: `src-tauri/src/main.rs`

Register in invoke_handler:
```rust
use crate::commands::my_feature_command;

fn main() {
    tauri::Builder::default()
        .manage(container)
        .invoke_handler(tauri::generate_handler![
            // ... existing commands
            my_feature_command,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

### Step 4: Test Command

Create test file: `src-tauri/tests/commands/my_feature_tests.rs`

```rust
use vault_desktop::commands::my_feature_command;
use vault_desktop::di::ServiceContainer;
use tauri::State;

#[tokio::test]
async fn test_my_feature_command_success() {
    // Setup: create test container with mocks
    let container = create_test_container().await;

    // Execute: call command
    let response = my_feature_command(
        State::from(&container),
        MyFeatureRequest {
            input: "test input".to_string(),
            options: Some(MyFeatureOptions {
                limit: Some(10),
                filter: None,
            }),
        },
    )
    .await
    .unwrap();

    // Verify: check response
    assert!(!response.result.is_empty());
    assert_eq!(response.count, 10);
}

#[tokio::test]
async fn test_my_feature_command_rate_limiting() {
    let container = create_test_container().await;

    // Execute: exhaust rate limit
    for _ in 0..100 {
        my_feature_command(State::from(&container), test_request()).await.ok();
    }

    // Verify: next request fails
    let result = my_feature_command(State::from(&container), test_request()).await;
    assert!(result.is_err());
}
```

### Step 5: Frontend Integration

**File**: `src/services/myFeatureService.ts`

```typescript
import { invoke } from '@tauri-apps/api/core';

interface MyFeatureRequest {
  input: string;
  options?: {
    limit?: number;
    filter?: string;
  };
}

interface MyFeatureResponse {
  result: string;
  count: number;
  executionTimeMs: number;
}

export async function performMyFeature(
  input: string,
  options?: MyFeatureRequest['options']
): Promise<MyFeatureResponse> {
  try {
    return await invoke<MyFeatureResponse>('my_feature_command', {
      request: { input, options },
    });
  } catch (error) {
    console.error('[MyFeature] Error:', error);
    throw new Error(`My feature failed: ${error}`);
  }
}
```

**File**: `src/components/MyFeature.tsx`

```typescript
import { useState } from 'react';
import { performMyFeature } from '../services/myFeatureService';

export function MyFeature() {
  const [input, setInput] = useState('');
  const [result, setResult] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async () => {
    setLoading(true);
    setError(null);

    try {
      const response = await performMyFeature(input, { limit: 10 });
      setResult(response.result);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unknown error');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div>
      <input
        value={input}
        onChange={(e) => setInput(e.target.value)}
        placeholder="Enter input..."
      />
      <button onClick={handleSubmit} disabled={loading}>
        {loading ? 'Processing...' : 'Submit'}
      </button>
      {error && <div className="error">{error}</div>}
      {result && <div className="result">{result}</div>}
    </div>
  );
}
```

---

## Architecture Quality Checks

### Before Adding New Code

**Always verify**:
1. Trait implementations are co-located with their types
2. Mocks have `#[cfg(test)]` guards
3. No production code in `mocks/` directories
4. No duplicate trait implementations

### Quick Verification Commands

```bash
# Check for missing #[cfg(test)] guards on mocks
cd src-tauri/src
rg "pub struct Mock" --type rust | grep -v "#\[cfg(test)\]"

# Check for production code in mock files
find . -path "*/mocks/*.rs" -exec grep -l "^impl.*Trait for [^Mock]" {} \;

# Verify all services have both trait and implementation
ls services/*.rs | while read f; do
  echo "Checking $f..."
  grep -q "impl.*Trait for" "$f" || echo "⚠️  No trait impl in $f"
done
```

**Expected Output**:
- First command: No output (all mocks properly guarded)
- Second command: No output (no production code in mocks)
- Third command: Each service file has implementation

### Common Architecture Violations

#### ❌ **Violation 1: Implementation in Wrong File**
```rust
// services/mocks/mock_search.rs - WRONG!
impl SearchServiceTrait for VectorSearchService { }
```

**Fix**: Move to `services/vector_search.rs`:
```rust
// services/vector_search.rs - CORRECT
impl SearchServiceTrait for VectorSearchService { }
```

#### ❌ **Violation 2: Missing `#[cfg(test)]` Guard**
```rust
// services/traits.rs - WRONG!
pub struct MockTagService { } // Missing guard

impl TagServiceTrait for MockTagService { } // Missing guard
```

**Fix**: Add guards:
```rust
// services/traits.rs - CORRECT
#[cfg(test)]
pub struct MockTagService { }

#[cfg(test)]
impl TagServiceTrait for MockTagService { }
```

#### ❌ **Violation 3: Production Code in Mock File**
```rust
// services/mocks/mock_web.rs - WRONG!
#![cfg(test)]

pub struct MockWebScraper { } // OK
pub struct WebScraperService { } // WRONG! Production code!
```

**Fix**: Create separate file:
```rust
// services/web_scraper.rs - CORRECT
pub struct WebScraperService { }
impl WebScraperServiceTrait for WebScraperService { }
```

---

## Adding a New Service

Services contain **business logic** and orchestrate between repositories and domain models.

### Step 1: Define Service Trait

**File**: `src-tauri/src/services/traits.rs`

Add your trait:
```rust
/// Trait for my feature service
///
/// Provides business logic for my feature operations.
#[async_trait]
pub trait MyServiceTrait: Send + Sync {
    /// Perform the core operation
    ///
    /// # Arguments
    /// * `input` - Validated input string
    ///
    /// # Returns
    /// Result with processed data
    ///
    /// # Errors
    /// - `AppError::InvalidInput` if input is invalid
    /// - `AppError::DatabaseError` if database operation fails
    async fn do_something(&self, input: &str) -> Result<ServiceResult>;

    /// Get status information
    async fn get_status(&self) -> Result<StatusInfo>;
}

// Supporting types
#[derive(Debug, Clone)]
pub struct ServiceResult {
    pub data: String,
    pub count: usize,
}

#[derive(Debug, Clone)]
pub struct StatusInfo {
    pub is_ready: bool,
    pub last_operation: Option<DateTime<Utc>>,
}
```

### Step 2: Add Mock Implementation

**File**: `src-tauri/src/services/traits.rs` (in same file)

```rust
/// Mock implementation of MyServiceTrait for testing
pub struct MockMyService {
    responses: Arc<Mutex<HashMap<String, ServiceResult>>>,
    should_fail: Arc<Mutex<bool>>,
}

impl MockMyService {
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(HashMap::new())),
            should_fail: Arc::new(Mutex::new(false)),
        }
    }

    /// Set expected response for input
    pub fn set_response(&self, input: &str, response: ServiceResult) {
        self.responses
            .lock()
            .unwrap()
            .insert(input.to_string(), response);
    }

    /// Configure service to fail
    pub fn set_should_fail(&self, should_fail: bool) {
        *self.should_fail.lock().unwrap() = should_fail;
    }
}

#[async_trait]
impl MyServiceTrait for MockMyService {
    async fn do_something(&self, input: &str) -> Result<ServiceResult> {
        if *self.should_fail.lock().unwrap() {
            return Err(AppError::ServiceError("Mock failure".into()));
        }

        Ok(self
            .responses
            .lock()
            .unwrap()
            .get(input)
            .cloned()
            .unwrap_or(ServiceResult {
                data: format!("mock result for {}", input),
                count: 1,
            }))
    }

    async fn get_status(&self) -> Result<StatusInfo> {
        Ok(StatusInfo {
            is_ready: true,
            last_operation: Some(Utc::now()),
        })
    }
}
```

### Step 3: Implement Production Service

**File**: `src-tauri/src/services/my_service.rs`

```rust
use crate::error::{AppError, Result};
use crate::repositories::MyRepository;
use crate::services::traits::*;
use async_trait::async_trait;
use sqlx::SqlitePool;
use std::sync::Arc;

/// Production implementation of MyServiceTrait
pub struct MyService {
    pool: SqlitePool,
    my_repository: Arc<MyRepository>,
    other_service: Arc<dyn OtherServiceTrait>,
}

impl MyService {
    /// Create new service instance
    pub fn new(
        pool: SqlitePool,
        my_repository: Arc<MyRepository>,
        other_service: Arc<dyn OtherServiceTrait>,
    ) -> Self {
        Self {
            pool,
            my_repository,
            other_service,
        }
    }

    /// Internal helper method
    async fn process_data(&self, input: &str) -> Result<String> {
        // Business logic implementation
        let data = self.my_repository.fetch_data(input).await?;
        let processed = self.other_service.transform(data).await?;
        Ok(processed)
    }
}

#[async_trait]
impl MyServiceTrait for MyService {
    async fn do_something(&self, input: &str) -> Result<ServiceResult> {
        // 1. Validate business rules
        if input.len() > 1000 {
            return Err(AppError::InvalidInput("Input too long".into()));
        }

        // 2. Process data
        let data = self.process_data(input).await?;

        // 3. Store results
        let count = self.my_repository.save_result(&data).await?;

        // 4. Return result
        Ok(ServiceResult { data, count })
    }

    async fn get_status(&self) -> Result<StatusInfo> {
        let last_op = self.my_repository.get_last_operation_time().await?;

        Ok(StatusInfo {
            is_ready: true,
            last_operation: last_op,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_my_service_success() {
        let service = create_test_service().await;

        let result = service.do_something("test input").await.unwrap();

        assert!(!result.data.is_empty());
        assert!(result.count > 0);
    }

    #[tokio::test]
    async fn test_my_service_validates_input() {
        let service = create_test_service().await;

        let result = service.do_something("x".repeat(1001).as_str()).await;

        assert!(result.is_err());
    }
}
```

### Step 4: Register in ServiceContainer

**File**: `src-tauri/src/di/service_container.rs`

Add field:
```rust
pub struct ServiceContainer {
    // ... existing fields
    my_service: Arc<dyn MyServiceTrait>,
}
```

Add constructor parameter:
```rust
impl ServiceContainer {
    pub fn new(
        // ... existing parameters
        my_service: Arc<dyn MyServiceTrait>,
    ) -> Self {
        Self {
            // ... existing fields
            my_service,
        }
    }

    /// Get my service instance
    pub fn my_service(&self) -> Arc<dyn MyServiceTrait> {
        Arc::clone(&self.my_service)
    }
}
```

### Step 5: Update Main Setup

**File**: `src-tauri/src/main.rs`

```rust
// Create service instance
let my_repository = Arc::new(MyRepository::new(pool.clone()));
let my_service = Arc::new(MyService::new(
    pool.clone(),
    my_repository,
    other_service.clone(),
)) as Arc<dyn MyServiceTrait>;

// Register in container
let container = ServiceContainer::new(
    // ... existing services
    my_service,
);
```

---

## Adding a New Domain Model

Domain models contain **business logic** and enforce **invariants**.

### Step 1: Create Domain Model

**File**: `src-tauri/src/domain/my_aggregate.rs`

```rust
use crate::domain_types::{MyAggregateId, MyEntityId};
use crate::error::{AppError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ============================================================================
// Aggregate Root
// ============================================================================

/// MyAggregate aggregate root
///
/// Encapsulates business entities and enforces invariants.
///
/// # Invariants
/// - Must have at least one entity
/// - Entity IDs must be unique within aggregate
/// - Status transitions follow business rules
#[derive(Debug, Clone)]
pub struct MyAggregate {
    id: MyAggregateId,
    entities: Vec<MyEntity>,
    status: AggregateStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl MyAggregate {
    /// Create new aggregate with validation
    ///
    /// # Errors
    /// Returns error if entities list is empty (invariant violation)
    pub fn new(id: MyAggregateId, entities: Vec<MyEntity>) -> Result<Self> {
        // Enforce invariant: must have at least one entity
        if entities.is_empty() {
            return Err(AppError::InvalidInput(
                "Aggregate must have at least one entity".into(),
            ));
        }

        // Enforce invariant: unique entity IDs
        let mut seen_ids = std::collections::HashSet::new();
        for entity in &entities {
            if !seen_ids.insert(entity.id().clone()) {
                return Err(AppError::InvalidInput(
                    format!("Duplicate entity ID: {:?}", entity.id()),
                ));
            }
        }

        let now = Utc::now();

        Ok(Self {
            id,
            entities,
            status: AggregateStatus::Active,
            created_at: now,
            updated_at: now,
        })
    }

    /// Add entity to aggregate
    ///
    /// # Errors
    /// Returns error if entity ID already exists (invariant)
    pub fn add_entity(&mut self, entity: MyEntity) -> Result<()> {
        // Check for duplicate ID
        if self.entities.iter().any(|e| e.id() == entity.id()) {
            return Err(AppError::InvalidInput(
                format!("Entity {:?} already exists", entity.id()),
            ));
        }

        self.entities.push(entity);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Remove entity by ID
    pub fn remove_entity(&mut self, id: &MyEntityId) -> Result<()> {
        let initial_len = self.entities.len();

        // Maintain invariant: must have at least one entity
        if initial_len == 1 {
            return Err(AppError::InvalidInput(
                "Cannot remove last entity from aggregate".into(),
            ));
        }

        self.entities.retain(|e| e.id() != id);

        if self.entities.len() == initial_len {
            return Err(AppError::NotFound(
                format!("Entity {:?} not found", id),
            ));
        }

        self.updated_at = Utc::now();
        Ok(())
    }

    /// Business logic: check if aggregate can be archived
    pub fn can_archive(&self) -> bool {
        matches!(self.status, AggregateStatus::Active)
            && self.entities.iter().all(|e| e.is_completed())
    }

    /// Business logic: archive aggregate
    ///
    /// # Errors
    /// Returns error if aggregate cannot be archived
    pub fn archive(&mut self) -> Result<()> {
        if !self.can_archive() {
            return Err(AppError::InvalidOperation(
                "Cannot archive aggregate in current state".into(),
            ));
        }

        self.status = AggregateStatus::Archived;
        self.updated_at = Utc::now();
        Ok(())
    }

    // Getters (read-only to preserve invariants)
    pub fn id(&self) -> &MyAggregateId {
        &self.id
    }

    pub fn entities(&self) -> &[MyEntity] {
        &self.entities
    }

    pub fn status(&self) -> &AggregateStatus {
        &self.status
    }
}

// ============================================================================
// Owned Entity
// ============================================================================

#[derive(Debug, Clone)]
pub struct MyEntity {
    id: MyEntityId,
    data: String,
    completed: bool,
}

impl MyEntity {
    pub fn new(id: MyEntityId, data: String) -> Self {
        Self {
            id,
            data,
            completed: false,
        }
    }

    pub fn id(&self) -> &MyEntityId {
        &self.id
    }

    pub fn is_completed(&self) -> bool {
        self.completed
    }

    pub fn mark_completed(&mut self) {
        self.completed = true;
    }
}

// ============================================================================
// Value Types
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AggregateStatus {
    Active,
    Archived,
    Deleted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aggregate_enforces_one_entity_minimum() {
        let result = MyAggregate::new(MyAggregateId::new_v4(), vec![]);

        assert!(result.is_err());
    }

    #[test]
    fn test_aggregate_prevents_duplicate_entity_ids() {
        let entity1 = MyEntity::new(MyEntityId::new_v4(), "data1".into());
        let entity2 = MyEntity::new(entity1.id().clone(), "data2".into());

        let result = MyAggregate::new(
            MyAggregateId::new_v4(),
            vec![entity1, entity2],
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_aggregate_cannot_remove_last_entity() {
        let entity = MyEntity::new(MyEntityId::new_v4(), "data".into());
        let mut aggregate =
            MyAggregate::new(MyAggregateId::new_v4(), vec![entity.clone()]).unwrap();

        let result = aggregate.remove_entity(entity.id());

        assert!(result.is_err());
    }

    #[test]
    fn test_aggregate_archive_business_logic() {
        let mut entity1 = MyEntity::new(MyEntityId::new_v4(), "data1".into());
        let mut entity2 = MyEntity::new(MyEntityId::new_v4(), "data2".into());
        entity1.mark_completed();
        entity2.mark_completed();

        let mut aggregate =
            MyAggregate::new(MyAggregateId::new_v4(), vec![entity1, entity2]).unwrap();

        assert!(aggregate.can_archive());
        assert!(aggregate.archive().is_ok());
        assert_eq!(*aggregate.status(), AggregateStatus::Archived);
    }
}
```

### Step 2: Register in Domain Module

**File**: `src-tauri/src/domain/mod.rs`

```rust
pub mod my_aggregate;

pub use my_aggregate::{MyAggregate, MyEntity, AggregateStatus};
```

### Step 3: Use in Service

```rust
impl MyService {
    async fn create_aggregate(&self, data: Vec<EntityData>) -> Result<MyAggregate> {
        // Create domain entities
        let entities: Vec<MyEntity> = data
            .into_iter()
            .map(|d| MyEntity::new(MyEntityId::new_v4(), d.content))
            .collect();

        // Create aggregate (enforces invariants)
        let aggregate = MyAggregate::new(MyAggregateId::new_v4(), entities)?;

        // Persist to database
        self.repository.save_aggregate(&aggregate).await?;

        Ok(aggregate)
    }
}
```

---

## Adding Security Controls

### Rate Limiting

**File**: `src-tauri/src/security/rate_limiter.rs`

Add new rate limiter:
```rust
pub struct RateLimiters {
    // ... existing limiters
    pub my_operation: RateLimiter,
}

impl Default for RateLimiters {
    fn default() -> Self {
        Self {
            // ... existing limiters
            my_operation: RateLimiter::new(50, Duration::from_secs(60)), // 50/min
        }
    }
}
```

Use in command:
```rust
container
    .security_context()
    .rate_limiters
    .my_operation
    .check()
    .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;
```

### Audit Logging

**File**: `src-tauri/src/audit/event.rs`

Add new action:
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditAction {
    // ... existing actions
    MyFeaturePerformed,
    MyEntityCreated,
    MyEntityDeleted,
}
```

Use in code:
```rust
use crate::audit::{audit_success, audit_failure, AuditAction};

// Success
audit_success!(
    action = AuditAction::MyFeaturePerformed,
    resource_type = "my_resource",
    resource_id = resource_id.as_str(),
    metadata = ("key", "value")
);

// Failure
audit_failure!(
    action = AuditAction::MyFeaturePerformed,
    resource_type = "my_resource",
    resource_id = resource_id.as_str(),
    error = error_message
);
```

### Input Validation

**File**: `src-tauri/src/security/input_validator.rs`

Add validation method:
```rust
impl InputValidator {
    /// Validate my feature input
    pub fn validate_my_input(&self, input: &str) -> Result<String> {
        // Length check
        if input.len() > MAX_INPUT_LENGTH {
            return Err(AppError::InvalidInput(
                format!("Input exceeds maximum length of {}", MAX_INPUT_LENGTH)
            ));
        }

        // Content check
        if input.trim().is_empty() {
            return Err(AppError::InvalidInput("Input cannot be empty".into()));
        }

        // Pattern check
        if contains_dangerous_patterns(input) {
            return Err(AppError::InvalidInput("Invalid input content".into()));
        }

        Ok(input.trim().to_string())
    }
}
```

Use in command:
```rust
let validated = container
    .security_context()
    .input_validator
    .validate_my_input(&request.input)?;
```

---

## Testing Guide

### Running Tests

```bash
# All tests
cargo test

# Unit tests only
cargo test --lib

# Integration tests only
cargo test --test '*'

# Specific test
cargo test test_my_feature

# With output
cargo test -- --nocapture

# With coverage
cargo tarpaulin --out Html --output-dir coverage
```

### Writing Unit Tests

**Domain Model Tests**:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invariant_enforcement() {
        let result = MyAggregate::new(id, vec![]); // Invalid: empty
        assert!(result.is_err());
    }

    #[test]
    fn test_business_logic() {
        let mut aggregate = create_test_aggregate();
        assert!(aggregate.can_archive());
    }
}
```

**Service Tests with Mocks**:
```rust
#[tokio::test]
async fn test_service_with_mock_dependency() {
    // Setup: create mocks
    let mock_dependency = Arc::new(MockDependency::new());
    mock_dependency.set_response("input", expected_result);

    // Create service with mock
    let service = MyService::new(test_pool(), mock_dependency);

    // Execute
    let result = service.do_something("input").await.unwrap();

    // Verify
    assert_eq!(result, expected_result);
}
```

### Writing Integration Tests

**Command Integration Tests**:
```rust
#[tokio::test]
async fn test_command_end_to_end() {
    // Setup: real container
    let container = create_test_container().await;

    // Execute: call command
    let response = my_command(
        State::from(&container),
        test_request(),
    ).await.unwrap();

    // Verify: check response
    assert!(response.is_valid());
}
```

### Writing Security Tests

```rust
#[tokio::test]
async fn test_rate_limiting() {
    let limiter = RateLimiter::new(10, Duration::from_secs(60));

    for _ in 0..10 {
        assert!(limiter.check().is_ok());
    }

    assert!(limiter.check().is_err());
}

#[test]
fn test_path_validation() {
    let malicious = "../../../etc/passwd";
    let result = ValidatedFilePath::new(PathBuf::from(malicious));
    assert!(result.is_err());
}
```

---

## Debugging Guide

### Enable Debug Logging

**File**: `src-tauri/src/main.rs`

```rust
env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
```

### Debug with VSCode

**File**: `.vscode/launch.json`

```json
{
  "version": "0.2.0",
  "configurations": [
    {
      "type": "lldb",
      "request": "launch",
      "name": "Debug Tauri App",
      "cargo": {
        "args": ["build", "--manifest-path=src-tauri/Cargo.toml"]
      },
      "args": [],
      "cwd": "${workspaceFolder}"
    }
  ]
}
```

### Common Debug Techniques

**Print Debugging**:
```rust
println!("DEBUG: value = {:?}", value);
tracing::debug!("Processing input: {:?}", input);
```

**Conditional Breakpoints** (use `dbg!` macro):
```rust
let result = service.do_something(input).await?;
dbg!(&result); // Prints and returns value
```

**Inspect Tauri State**:
```rust
#[tauri::command]
async fn debug_state(container: State<'_, ServiceContainer>) -> Result<String> {
    Ok(format!("{:?}", container.inner()))
}
```

---

## Common Patterns

### Pattern: Batch Processing with Rate Limiting

```rust
#[tauri::command]
pub async fn batch_operation(
    container: State<'_, ServiceContainer>,
    items: Vec<String>,
) -> Result<BatchResponse> {
    // Validate batch size (DoS protection)
    const MAX_BATCH_SIZE: usize = 100;
    if items.len() > MAX_BATCH_SIZE {
        return Err(AppError::InvalidInput(
            format!("Batch size {} exceeds maximum {}", items.len(), MAX_BATCH_SIZE)
        ));
    }

    // Rate limiting
    container
        .security_context()
        .rate_limiters
        .batch_operations
        .check()?;

    // Process with progress tracking
    let mut results = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let result = process_item(container.clone(), item).await?;
        results.push(result);

        // Emit progress
        emit_progress(index + 1, items.len());
    }

    Ok(BatchResponse { results })
}
```

### Pattern: Transaction Management

```rust
impl MyService {
    pub async fn transactional_operation(&self, data: Data) -> Result<()> {
        // Start transaction
        let mut tx = self.pool.begin().await?;

        // Perform operations
        self.repository.insert(&mut tx, &data).await?;
        self.other_repository.update(&mut tx, &data.id).await?;

        // Commit (or rollback on error)
        tx.commit().await?;

        Ok(())
    }
}
```

### Pattern: Retry with Exponential Backoff

```rust
use crate::utils::retry::{retry_with_backoff, RetryConfig};

async fn operation_with_retry() -> Result<Data> {
    let config = RetryConfig {
        max_attempts: 3,
        initial_delay: Duration::from_millis(100),
        max_delay: Duration::from_secs(5),
    };

    retry_with_backoff(config, || async {
        // Operation that might fail
        external_service.fetch_data().await
    }).await
}
```

---

## Troubleshooting

### Common Issues

#### "Service not found in container"

**Problem**: Trying to access service that wasn't registered

**Solution**: Check ServiceContainer initialization in main.rs
```rust
let container = ServiceContainer::new(
    // ... make sure all services are passed
    my_service,  // Don't forget this!
);
```

#### "Rate limit exceeded" in tests

**Problem**: Tests hitting rate limits

**Solution**: Use mock services or reset rate limiters between tests
```rust
#[tokio::test]
async fn test_with_fresh_container() {
    let container = create_test_container().await; // Fresh container each test
    // ...
}
```

#### "Invariant violation" errors

**Problem**: Domain model invariant check failed

**Solution**: Check domain model creation logic
```rust
// Bad: empty entities
MyAggregate::new(id, vec![]) // Error: needs ≥1 entity

// Good: valid entities
MyAggregate::new(id, vec![entity1, entity2])
```

#### "Database locked" errors

**Problem**: SQLite concurrent access

**Solution**: Use connection pooling and transactions properly
```rust
// Set up pool with proper limits
SqlitePoolOptions::new()
    .max_connections(5)
    .connect(&database_url)
    .await?;
```

### Getting Help

- **Architecture Questions**: See [ARCHITECTURE.md](ARCHITECTURE.md)
- **API Documentation**: Run `cargo doc --open`
- **Test Examples**: Check `src-tauri/tests/`
- **Domain Examples**: See `src-tauri/src/domain/`

---

## Summary Checklist

When adding new features, verify:

- [ ] **Command** created in `commands/`
- [ ] **Service trait** defined in `services/traits.rs`
- [ ] **Mock service** implemented for testing
- [ ] **Production service** implemented in `services/`
- [ ] **Domain model** created if needed (in `domain/`)
- [ ] **Repository** created if needed (in `repositories/`)
- [ ] **Rate limiting** applied to resource-intensive ops
- [ ] **Audit logging** added to security-sensitive ops
- [ ] **Input validation** on all user inputs
- [ ] **Unit tests** for domain/services
- [ ] **Integration tests** for commands
- [ ] **Security tests** for controls
- [ ] **Frontend integration** via `invoke()`
- [ ] **Documentation** added (inline docs)
- [ ] **Registered** in ServiceContainer
- [ ] **Registered** in main.rs invoke_handler

---

**Version**: 1.0
**Last Updated**: 2025-01-17
**Maintainer**: Development Team
