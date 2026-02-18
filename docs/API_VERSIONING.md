# API Versioning Policy

**Version**: 1.0
**Last Updated**: 2025-11-14
**Current API Version**: v1

## Philosophy

Recall's API versioning follows these principles:

1. **Stability over agility** - API changes are rare and deliberate
2. **Local-first compatibility** - Clients control when they upgrade
3. **Pragmatic versioning** - No premature complexity for theoretical problems
4. **Clear migration paths** - When changes happen, migration is straightforward

Since Recall is a local-first application where users run both client and server, we can be more flexible than cloud APIs. However, we maintain versioning discipline for:
- Third-party integrations
- Plugin developers
- Future multi-user scenarios
- Backward compatibility guarantees

---

## Versioning Scheme

### URL-Based Versioning

All API endpoints use URL-based versioning:

```
/api/v1/search
/api/v1/documents
/api/v1/health
```

**Rationale**: URL-based versioning is explicit, cache-friendly, and works seamlessly with API documentation tools.

### Version Format

- **Major version only** (v1, v2, v3)
- No minor/patch versions in URLs
- Semantic changes tracked internally via app version

**Current version**: `v1`

---

## What Requires a Version Bump

### Breaking Changes (Require New Major Version)

These changes **require** incrementing the major version (v1 → v2):

| Change Type | Example | Impact |
|-------------|---------|--------|
| **Removing endpoints** | DELETE `/api/v1/search` | Client requests fail |
| **Removing required fields** | Remove `query` from search request | Client requests rejected |
| **Removing response fields** | Remove `score` from search results | Client parsing breaks |
| **Changing field types** | `timestamp: int` → `timestamp: string` | Type errors in clients |
| **Changing field names** | `content` → `body` | Client parsing breaks |
| **Changing authentication** | Basic Auth → OAuth2 only | Auth failures |
| **Changing error formats** | Different error response structure | Error handling breaks |
| **Changing HTTP methods** | GET `/api/v1/documents` → POST | Wrong method errors |
| **Changing required parameters** | Add required `filter` param | Requests rejected |
| **Changing semantics** | Search now requires exact match | Behavior changes unexpectedly |

### Non-Breaking Changes (Same Version)

These changes are **safe** within the same version:

| Change Type | Example | Why Safe |
|-------------|---------|----------|
| **Adding endpoints** | POST `/api/v1/export` | Clients ignore new endpoints |
| **Adding optional fields** | Add optional `limit` param | Defaults handle omission |
| **Adding response fields** | Add `embedding_model` to results | Clients ignore unknown fields |
| **Relaxing validation** | Allow 0-1000 results instead of 1-100 | Expands possibilities |
| **Performance improvements** | Faster search algorithm | Transparent to clients |
| **Bug fixes** | Fix incorrect ranking | Corrects unintended behavior |
| **Adding new error codes** | Add `413 Payload Too Large` | Clients handle via status codes |
| **Documentation updates** | Clarify field descriptions | Non-functional |

### Gray Areas (Evaluate Case-by-Case)

| Change Type | Decision Criteria |
|-------------|-------------------|
| **Changing defaults** | If clients rely on implicit defaults → breaking |
| **Tightening validation** | If currently-valid requests become invalid → breaking |
| **Deprecating features** | Follow deprecation process (see below) |
| **Changing rate limits** | Generally non-breaking unless drastic reduction |

---

## Version Lifecycle

### Support Timeline

```
v1 Released ────────────────────────────────────────────►
                  v2 Released ──────────────────────────►
                           │◄── 1 year ──►│
                           Overlap period  v1 EOL
```

- **Current version**: Actively developed, receives all updates
- **Previous version**: Supported for **1 year** after next version release
  - Security fixes: Yes
  - Bug fixes: Critical only
  - New features: No
- **End-of-life**: Version removed, requests return 410 Gone

### Current Status

| Version | Status | Release Date | EOL Date | Notes |
|---------|--------|--------------|----------|-------|
| v1 | **Active** | 2025-01-15 | TBD | Current stable version |

---

## Deprecation Process

### Timeline

```
T+0:   Deprecation announced
T+3:   Warning headers added
T+6:   Sunset headers added
T+9:   Migration guide published
T+12:  Version deprecated (still works)
       New version released (v2)
T+24:  Version reaches EOL (returns 410)
```

### Step-by-Step Process

#### 1. Deprecation Announcement (T+0 months)

- Add notice to API documentation
- Update changelog with deprecation warning
- Publish migration timeline

#### 2. Warning Phase (T+3 months)

Add deprecation warning header to responses:

```http
HTTP/1.1 200 OK
X-API-Deprecated-Warning: This endpoint will be removed in v2. Migrate to /api/v2/new-endpoint
Link: </api/docs/migration>; rel="migration-guide"
```

#### 3. Sunset Phase (T+6 months)

Add RFC 8594 Sunset header:

```http
HTTP/1.1 200 OK
Sunset: Sat, 15 Jan 2027 00:00:00 GMT
X-API-Deprecated-Warning: This API version will be sunset on 2027-01-15
Link: </api/docs/migration>; rel="migration-guide"
```

#### 4. Migration Guide (T+9 months)

Publish comprehensive migration documentation:
- Breaking changes summary
- Code migration examples
- Testing checklist
- Rollback procedures

#### 5. Deprecated Status (T+12 months)

- New version (v2) released
- Old version (v1) enters 1-year support window
- No new features added to v1

#### 6. End-of-Life (T+24 months)

- API returns `410 Gone` for all v1 requests:

```http
HTTP/1.1 410 Gone
Content-Type: application/json

{
  "error": "api_version_eol",
  "message": "API v1 reached end-of-life on 2027-01-15",
  "migration_guide": "/api/docs/migration/v1-to-v2",
  "current_version": "v2"
}
```

---

## Client Migration Guide

### Handling Version Changes

#### 1. Version Detection

Always check API version in responses:

```python
import httpx

response = httpx.get("http://localhost:8000/api/v1/health")
api_version = response.headers.get("X-API-Version", "v1")
```

#### 2. Monitoring Deprecation Warnings

Check for deprecation headers:

```python
def check_deprecation(response):
    if "X-API-Deprecated-Warning" in response.headers:
        warning = response.headers["X-API-Deprecated-Warning"]
        logger.warning(f"API Deprecation: {warning}")

    if "Sunset" in response.headers:
        sunset_date = response.headers["Sunset"]
        logger.error(f"API will sunset on: {sunset_date}")
```

#### 3. Testing Against New Versions

Before migrating, test against the new version:

```python
class APIClient:
    def __init__(self, base_url: str, version: str = "v1"):
        self.base_url = base_url
        self.version = version

    def search(self, query: str):
        url = f"{self.base_url}/api/{self.version}/search"
        return httpx.post(url, json={"query": query})

client_v1 = APIClient("http://localhost:8000", version="v1")
client_v2 = APIClient("http://localhost:8000", version="v2")

results_v1 = client_v1.search("test")
results_v2 = client_v2.search("test")
assert results_v1 == results_v2  # Verify compatibility
```

#### 4. Gradual Migration Strategy

Use feature flags to toggle between versions:

```python
import os

API_VERSION = os.getenv("RECALL_API_VERSION", "v1")

client = APIClient(
    base_url="http://localhost:8000",
    version=API_VERSION
)
```

#### 5. Rollback Procedures

If issues arise after migration:

1. **Immediate rollback**: Change `API_VERSION` environment variable
2. **Report issue**: Create GitHub issue with reproduction steps
3. **Stay on old version**: Continue using v1 until issue resolved
4. **Re-test**: Verify fix before re-attempting migration

---

## Migration Examples

### Example 1: Field Rename (Breaking Change)

**Scenario**: In v2, `Document.content` renamed to `Document.body`

**v1 Code**:
```python
response = httpx.get("/api/v1/documents/123")
content = response.json()["content"]
```

**v2 Migration**:
```python
response = httpx.get("/api/v2/documents/123")
body = response.json()["body"]  # Field renamed
```

**Backward-Compatible Wrapper**:
```python
def get_document_content(doc_id: str, version: str = "v1") -> str:
    response = httpx.get(f"/api/{version}/documents/{doc_id}")
    data = response.json()

    if version == "v1":
        return data["content"]
    else:  # v2+
        return data["body"]
```

### Example 2: New Optional Parameter (Non-Breaking)

**Scenario**: v1 adds optional `include_embeddings` parameter

**Old Code (Still Works)**:
```python
response = httpx.post("/api/v1/search", json={
    "query": "machine learning"
})
```

**New Code (Using New Feature)**:
```python
response = httpx.post("/api/v1/search", json={
    "query": "machine learning",
    "include_embeddings": True  # Optional, defaults to False
})
```

### Example 3: Authentication Change (Breaking Change)

**Scenario**: v2 requires API key authentication

**v1 Code (No Auth)**:
```python
response = httpx.post("/api/v1/search", json={"query": "test"})
```

**v2 Migration (API Key Required)**:
```python
headers = {"X-API-Key": os.getenv("RECALL_API_KEY")}
response = httpx.post(
    "/api/v2/search",
    json={"query": "test"},
    headers=headers
)
```

---

## Version-Specific Behavior

### Response Headers

All API responses include version information:

```http
HTTP/1.1 200 OK
X-API-Version: v1
X-App-Version: 0.1.0
Content-Type: application/json
```

### Error Responses

Consistent error format across all versions:

```json
{
  "error": "validation_error",
  "message": "Query parameter is required",
  "details": {
    "field": "query",
    "constraint": "required"
  },
  "timestamp": "2025-11-14T10:30:00Z",
  "api_version": "v1"
}
```

### Health Check Endpoint

Always available at `/api/{version}/health`:

```bash
curl http://localhost:8000/api/v1/health
```

```json
{
  "status": "healthy",
  "version": "v1",
  "app_version": "0.1.0",
  "timestamp": "2025-11-14T10:30:00Z"
}
```

---

## API Stability Guarantees

### What We Promise

1. **No silent breaking changes**: If it breaks clients, it gets a new version
2. **1-year overlap**: Old version supported for 1 year after new release
3. **Clear migration guides**: Step-by-step instructions for all breaking changes
4. **Deprecation warnings**: Minimum 12-month notice before EOL
5. **Semantic stability**: Endpoints behave consistently within a version

### What We Don't Promise

1. **Unlimited backward compatibility**: Old versions will eventually EOL
2. **Performance SLAs**: Response times may vary with improvements
3. **Immutable bugs**: Bug fixes may change behavior (for the better)
4. **Frozen features**: We may add features within a version

---

## Special Considerations for Local-First

Since Recall is local-first, some versioning considerations are unique:

### Database Schema Versions

API version is **separate** from database schema version:
- API v1 may work with multiple DB schema versions
- Schema migrations are automatic on startup
- No manual intervention required

### Client-Server Version Matching

Recommended: Keep client and server on same version.

Supported scenarios:
- ✅ Client v1 → Server v1
- ⚠️ Client v1 → Server v2 (during migration period)
- ❌ Client v2 → Server v1 (not supported)

### Plugin Compatibility

Plugins should specify compatible API versions:

```python
# plugin.py
class MyPlugin:
    api_version = "v1"
    min_api_version = "v1"
    max_api_version = "v2"
```

---

## Changelog Integration

API version changes are tracked in `CHANGELOG.md`:

```markdown
## [v2.0.0] - 2027-01-15

### Breaking Changes (API v2)
- **BREAKING**: Renamed `Document.content` to `Document.body`
- **BREAKING**: Search endpoint now requires `query` parameter
- **BREAKING**: Authentication required for all endpoints

### Migration
See docs/migrations/v1-to-v2.md for migration guide.

## [v1.5.0] - 2026-06-15

### Added (API v1)
- Added optional `include_embeddings` parameter to search
- Added `/api/v1/export` endpoint

### Fixed (API v1)
- Fixed search ranking for multi-word queries
```

---

## Decision Framework

When making API changes, ask:

1. **Will existing clients break?**
   - Yes → Breaking change → New version
   - No → Continue

2. **Does behavior change in unexpected ways?**
   - Yes → Breaking change → New version
   - No → Continue

3. **Can clients safely ignore the change?**
   - Yes → Non-breaking → Same version
   - No → Breaking change → New version

**When in doubt**: Treat as breaking change. It's easier to be conservative.

---

## Version Planning

### Criteria for v2

We will release v2 when:
- Accumulated breaking changes justify new version
- Architecture improvements require API redesign
- Authentication/security model changes
- Community requests major features requiring incompatibility

**NOT** when:
- Adding optional features
- Fixing bugs
- Improving performance
- Refactoring internals

### Pre-Release Testing

Before releasing new versions:

1. **Beta period**: 2-month beta with `-beta` suffix
2. **Migration guide**: Complete before beta release
3. **Community testing**: Gather feedback from plugin developers
4. **Automated tests**: Version compatibility test suite
5. **Documentation**: Update all docs before GA release

---

## References

- RFC 8594: Sunset HTTP Header
- Semantic Versioning: https://semver.org/
- API Evolution Without Versioning: https://www.mnot.net/blog/2012/12/04/api-evolution
- Stripe API Versioning: https://stripe.com/blog/api-versioning

---

## Maintenance

This document is reviewed:
- Before each major release
- Annually on January 15
- When versioning issues arise

**Document Owner**: Core Team
**Last Review**: 2025-11-14
**Next Review**: 2026-01-15
