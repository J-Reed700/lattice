# ADR-005: Use Newtype Pattern for Domain Entity IDs

## Status

**Accepted** - Implemented in Phase 1 (2025-11-15)

## Context

The Recall desktop application handles multiple domain entities with unique identifiers:

- **Documents**: Indexed text files, PDFs, etc.
- **Embeddings**: 384-dimensional vectors associated with documents
- **Collections**: User-defined groups of documents
- **Search queries**: Logged user searches
- **Users**: In future multi-user support

Traditional approaches to ID typing:

```rust
// Primitive types (error-prone)
fn get_document(id: String) -> Result<Document>;
fn get_embedding(id: String) -> Result<Embedding>;

// Problem: Easy to mix up IDs
get_document(embedding_id);  // Compiles but wrong!
```

**Key requirements**:

1. **Type safety**: Prevent mixing up IDs from different entity types
2. **Zero runtime cost**: No performance overhead vs raw strings
3. **Serialization**: Work seamlessly with Serde, SQLx, and Tauri IPC
4. **Ergonomics**: Minimal boilerplate for common operations
5. **Display/Debug**: Clear error messages and logging
6. **Comparison**: Support equality checks and hashing for collections

## Decision

We will use the **newtype pattern** with `derive_more` to create type-safe, zero-cost ID wrappers.

### Implementation

```rust
use derive_more::{Display, From, Into, AsRef, Deref};
use serde::{Serialize, Deserialize};

/// Type-safe document ID
#[derive(
    Debug, Clone, PartialEq, Eq, Hash,
    Display, From, Into, AsRef, Deref,
    Serialize, Deserialize
)]
#[serde(transparent)]
pub struct DocumentId(String);

impl DocumentId {
    /// Create a new random document ID
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// Create from an existing string (validation happens here)
    pub fn from_string(s: String) -> Result<Self, IdError> {
        if s.is_empty() {
            return Err(IdError::Empty);
        }
        Ok(Self(s))
    }

    /// Get the inner string value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// SQLx integration
impl sqlx::Type<sqlx::Sqlite> for DocumentId {
    fn type_info() -> <sqlx::Sqlite as sqlx::Database>::TypeInfo {
        <String as sqlx::Type<sqlx::Sqlite>>::type_info()
    }
}

impl<'q> sqlx::Encode<'q, sqlx::Sqlite> for DocumentId {
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::Sqlite as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        self.0.encode_by_ref(buf)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for DocumentId {
    fn decode(
        value: <sqlx::Sqlite as sqlx::Database>::ValueRef<'r>,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        Ok(Self(String::decode(value)?))
    }
}
```

### Usage Examples

```rust
// Type safety prevents errors
fn get_document(id: DocumentId) -> Result<Document>;
fn get_embedding(id: EmbeddingId) -> Result<Embedding>;

let doc_id = DocumentId::new();
let emb_id = EmbeddingId::new();

get_document(doc_id);      // ✅ OK
get_document(emb_id);      // ❌ Compile error: type mismatch!

// Transparent serialization
#[derive(Serialize)]
struct Response {
    id: DocumentId,
    title: String,
}

// JSON: {"id": "550e8400-e29b-41d4-a716-446655440000", "title": "..."}
```

### Complete Type Hierarchy

```rust
// Core entity IDs
pub struct DocumentId(String);
pub struct EmbeddingId(String);
pub struct CollectionId(String);
pub struct QueryId(String);

// Future: User and organization IDs
pub struct UserId(String);
pub struct OrgId(String);

// Shared ID traits
pub trait EntityId:
    Display + From<String> + Into<String> +
    AsRef<str> + Deref<Target=str> +
    Serialize + for<'de> Deserialize<'de>
{
    fn new() -> Self;
    fn from_string(s: String) -> Result<Self, IdError>;
}
```

## Consequences

### Positive

1. **Compile-time type safety**: Cannot mix up entity IDs
   ```rust
   // Compile error instead of runtime bug
   document_repo.get(embedding_id);  // ❌ Type error
   ```

2. **Zero runtime cost**: Newtype is erased at compile time
   ```rust
   assert_eq!(
       std::mem::size_of::<DocumentId>(),
       std::mem::size_of::<String>()
   );
   ```

3. **Ergonomic conversions**: `derive_more` eliminates boilerplate
   ```rust
   let id: DocumentId = "abc123".to_string().into();
   let s: String = id.into();
   let str_ref: &str = id.as_ref();
   ```

4. **Transparent serialization**: Serde treats newtype as inner type
   ```json
   // No wrapper object, just the ID string
   {"id": "550e8400-e29b-41d4-a716-446655440000"}
   ```

5. **Database integration**: SQLx handles newtype natively
   ```rust
   sqlx::query_as!(
       Document,
       "SELECT id, title FROM documents WHERE id = ?",
       doc_id  // Automatically converts to String
   )
   ```

6. **Clear error messages**: Compiler tells you exactly which type is wrong
   ```
   error[E0308]: mismatched types
    --> src/main.rs:42:25
     |
   42 |     get_document(emb_id);
     |                  ^^^^^^ expected `DocumentId`, found `EmbeddingId`
   ```

7. **Self-documenting code**: Function signatures are explicit
   ```rust
   // Clear intent
   fn link_embedding(doc_id: DocumentId, emb_id: EmbeddingId);

   // vs ambiguous
   fn link_embedding(doc_id: String, emb_id: String);
   ```

### Negative

1. **Conversion verbosity**: Need explicit `.into()` or `.as_ref()` calls
   ```rust
   let id = DocumentId::new();
   some_function(&id);  // ❌ May need: some_function(id.as_ref())
   ```

2. **Pattern matching complexity**: Need to unwrap in some contexts
   ```rust
   match doc_id {
       DocumentId(inner) => println!("{}", inner),  // Explicit unwrap
   }
   ```

3. **Library compatibility**: Some third-party libraries may not accept newtypes
   - **Mitigation**: Use `.as_ref()` or `.into()` to convert

4. **Derives proliferation**: Need many derived traits for full functionality
   ```rust
   #[derive(Debug, Clone, PartialEq, Eq, Hash, ...)]  // 10+ traits
   ```

5. **Testing complexity**: Need to construct typed IDs in tests
   ```rust
   // Before: just use a string
   let id = "test-id";

   // After: wrap in newtype
   let id = DocumentId::from_string("test-id".into()).unwrap();
   ```

### Neutral

- **Learning curve**: Team needs to understand newtype pattern (one-time cost)
- **Macro dependency**: Relies on `derive_more` crate (but it's stable and popular)

## Alternatives Considered

### 1. Type Aliases

```rust
type DocumentId = String;
type EmbeddingId = String;
```

**Pros**:
- Zero boilerplate
- Direct compatibility with all String APIs

**Cons**:
- **Rejected**: No type safety at all, aliases are transparent to the compiler
- `DocumentId` and `EmbeddingId` are interchangeable
- No prevention of ID confusion bugs

### 2. Enum-Based IDs

```rust
enum EntityId {
    Document(String),
    Embedding(String),
    Collection(String),
}
```

**Pros**:
- Single ID type can represent any entity
- Pattern matching on entity type

**Cons**:
- **Rejected**: Runtime overhead (8-byte discriminant)
- Awkward API: `EntityId::Document(id)` everywhere
- Cannot enforce correct type at compile time
- Larger memory footprint

### 3. Phantom Type Parameters

```rust
struct Id<T> {
    value: String,
    _marker: PhantomData<T>,
}

type DocumentId = Id<Document>;
type EmbeddingId = Id<Embedding>;
```

**Pros**:
- Type safety with generic implementation
- Single struct definition

**Cons**:
- **Rejected**: Complex error messages with generic types
- Requires `PhantomData` boilerplate
- Less clear intent than dedicated newtypes
- Harder to implement trait bounds

### 4. UUID-Based IDs

```rust
pub struct DocumentId(uuid::Uuid);
```

**Pros**:
- Globally unique, no collisions
- Fixed size (16 bytes vs variable String)

**Cons**:
- **Rejected**: Less flexible than strings (cannot use external IDs)
- Harder to debug (long hex strings)
- No semantic meaning
- We already use UUIDs *inside* the string representation

### 5. Integer IDs

```rust
pub struct DocumentId(i64);
```

**Pros**:
- Smallest size (8 bytes)
- Fast comparison and hashing
- Database-friendly (auto-increment)

**Cons**:
- **Rejected**: Not globally unique (requires coordination)
- Migration complexity if database changes
- Cannot represent external IDs (user-provided paths, URLs)

## Implementation Guidelines

### Creating New ID Types

```rust
use crate::ids::macros::define_id;

// Macro to reduce boilerplate (future improvement)
define_id!(DocumentId);
define_id!(EmbeddingId);

// Or manually:
#[derive(
    Debug, Clone, PartialEq, Eq, Hash,
    Display, From, Into, AsRef, Deref,
    Serialize, Deserialize
)]
#[serde(transparent)]
pub struct NewEntityId(String);

impl NewEntityId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}
```

### Testing with IDs

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Helper for tests
    impl DocumentId {
        pub fn test_id(value: &str) -> Self {
            Self(value.to_string())
        }
    }

    #[test]
    fn test_get_document() {
        let id = DocumentId::test_id("doc-123");
        let doc = get_document(id).unwrap();
        assert_eq!(doc.id, DocumentId::test_id("doc-123"));
    }
}
```

### Validation Logic

```rust
impl DocumentId {
    const MAX_LENGTH: usize = 256;

    pub fn from_string(s: String) -> Result<Self, IdError> {
        if s.is_empty() {
            return Err(IdError::Empty);
        }
        if s.len() > Self::MAX_LENGTH {
            return Err(IdError::TooLong);
        }
        if !s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return Err(IdError::InvalidCharacters);
        }
        Ok(Self(s))
    }
}
```

## Performance Considerations

**Memory layout**:
```rust
// All identical in memory (verified with assert_eq! above)
size_of::<DocumentId>()  == 24 bytes  (String = 3 * 8)
size_of::<String>()      == 24 bytes
size_of::<&str>()        == 16 bytes  (fat pointer)
```

**Zero-cost abstraction verified**:
```rust
#[test]
fn zero_cost_newtype() {
    let string = "test-id".to_string();
    let newtype = DocumentId::from_string(string.clone()).unwrap();

    // Same size
    assert_eq!(
        std::mem::size_of_val(&string),
        std::mem::size_of_val(&newtype)
    );

    // Same alignment
    assert_eq!(
        std::mem::align_of_val(&string),
        std::mem::align_of_val(&newtype)
    );
}
```

## Future Enhancements

1. **ID generation strategies**: Support different formats (ULID, Snowflake, etc.)
2. **Validation at creation**: Enforce ID format rules
3. **Namespace prefixes**: `doc_`, `emb_`, `col_` for human-readable debugging
4. **Macro for boilerplate**: `define_id!(DocumentId)` to reduce repetition

## References

- [Rust Newtype Pattern](https://doc.rust-lang.org/rust-by-example/generics/new_types.html)
- [derive_more crate](https://docs.rs/derive_more/)
- [Type-Driven Design in Rust](https://www.youtube.com/watch?v=z-0-bbc80JM)

## Revision History

- **2025-11-15**: Initial decision, implemented in Rust modernization Phase 1
