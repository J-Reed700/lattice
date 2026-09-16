# Architecture Decision Records (ADRs)

This directory contains Architecture Decision Records (ADRs) documenting significant architectural decisions made for the Lattice desktop application (Tauri/Rust backend).

## What are ADRs?

Architecture Decision Records capture important architectural decisions along with their context and consequences. They serve as a historical record of why certain technical choices were made.

## Format

Each ADR follows this structure:

- **Status**: Proposed, Accepted, Deprecated, Superseded
- **Context**: What is the issue we're seeing that is motivating this decision?
- **Decision**: What is the change we're proposing and/or doing?
- **Consequences**: What becomes easier or more difficult to do because of this change?
- **Alternatives Considered**: What other options were evaluated?

## Index

### Core Infrastructure

- [ADR-001: Use HNSW for Vector Search](./ADR-001-use-hnsw-for-vector-search.md)
  - Decision to use Hierarchical Navigable Small World graphs via `instant-distance` for ANN search
  - Status: **Accepted**

- [ADR-002: Use SQLite for Metadata Storage](./ADR-002-sqlite-for-metadata-storage.md)
  - Decision to use SQLite (via SQLx) as the embedded database for local-first storage
  - Status: **Accepted**

- [ADR-003: Use ONNX Runtime for Embeddings](./ADR-003-onnx-runtime-for-embeddings.md)
  - Decision to use ONNX Runtime for local embedding generation with quantized models
  - Status: **Accepted**

### Application Architecture

- [ADR-004: Use Tauri for Desktop App](./ADR-004-tauri-for-desktop-app.md)
  - Decision to use Tauri 2.0 as the cross-platform desktop framework
  - Status: **Accepted**

### Design Patterns

- [ADR-005: Use Newtype Pattern for Domain IDs](./ADR-005-newtype-pattern-for-domain-ids.md)
  - Decision to use newtype pattern with `derive_more` for type-safe entity IDs
  - Status: **Accepted**

## Creating New ADRs

When making a significant architectural decision:

1. Copy the template below or use an existing ADR as reference
2. Number it sequentially (ADR-XXX)
3. Use a descriptive filename: `ADR-XXX-short-title.md`
4. Fill in all sections
5. Update this README with a link to the new ADR
6. Commit the ADR with your changes

### ADR Template

```markdown
# ADR-XXX: [Title]

## Status

**[Proposed|Accepted|Deprecated|Superseded]** - [Date]

## Context

[What is the issue we're facing? What are the requirements?]

## Decision

[What are we doing? Include implementation details.]

## Consequences

### Positive

- [Benefit 1]
- [Benefit 2]

### Negative

- [Drawback 1]
- [Drawback 2]

### Neutral

- [Trade-off 1]

## Alternatives Considered

### 1. [Alternative Name]

**Pros**:
- [Pro 1]

**Cons**:
- [Con 1]
- **Rejected**: [Main reason]

## References

- [Link to relevant documentation]

## Revision History

- **YYYY-MM-DD**: Initial decision
```

## When to Write an ADR

Write an ADR when:

- Choosing a major technology or framework
- Changing a core architectural pattern
- Making a decision that will be hard to reverse
- Selecting between multiple viable options
- Introducing a significant new dependency

Don't write an ADR for:

- Minor implementation details
- Temporary fixes or experiments
- Decisions that can easily be changed
- Obvious or uncontested choices

## Related Documentation

- [Tauri Architecture](../architecture.md)
- [Contribution guide](../../../../../CONTRIBUTING.md)
- [Backend README](../../../README.md)

## References

- [ADR GitHub Org](https://adr.github.io/) - Resources and examples
- [Documenting Architecture Decisions](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions) - Original ADR blog post by Michael Nygard

---

**Last Updated**: 2025-11-15
