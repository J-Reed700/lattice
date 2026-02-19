# API Versioning Strategy

## Current Version

**Version:** v1  
**Status:** Stable  
**Base Path:** `/api/v1`

## Versioning Scheme

We use **URL path versioning** for its simplicity and explicit nature:

```
/api/v1/search
/api/v1/files
/api/v1/documents
```

### Why URL Path Versioning?

- **Explicit**: Version is immediately visible in the URL
- **Simple**: Easy to route and cache
- **Standard**: Widely adopted in REST APIs
- **Client-friendly**: Easy to update incrementally

## Version Lifecycle

### Active Development (v1 - Current)

- All new features added to v1
- Backward-compatible changes preferred
- Breaking changes require deprecation period
- Optional fields can be added freely
- Required fields require new endpoints or version bump

### Adding Non-Breaking Changes

✅ **Allowed without version bump:**
- Adding new optional request fields
- Adding new response fields
- Adding new endpoints
- Relaxing validation rules
- Improving error messages

❌ **Requires version bump:**
- Removing endpoints
- Removing request/response fields
- Changing field types
- Stricter validation rules
- Changing HTTP methods (e.g., POST → PUT)

## Deprecation Policy

### Timeline

1. **Announce deprecation**: Add deprecation headers immediately
2. **Sunset period**: Minimum 6 months support after announcement
3. **Remove**: Only after sunset date

### Deprecation Headers

When deprecating an endpoint, add these headers:

```http
Deprecation: true
Sunset: Sat, 31 Dec 2024 23:59:59 GMT
Link: </api/v2/endpoint>; rel="successor-version"
```

Example implementation:

```python
from fastapi import Response

@router.get("/old-endpoint")
async def deprecated_endpoint(response: Response):
    response.headers["Deprecation"] = "true"
    response.headers["Sunset"] = "Sat, 31 Dec 2024 23:59:59 GMT"
    response.headers["Link"] = '</api/v2/new-endpoint>; rel="successor-version"'
    return {"message": "This endpoint is deprecated"}
```

## Breaking Change Policy

### What Constitutes a Breaking Change?

- Removing or renaming fields
- Changing field types
- Changing HTTP status codes
- Changing authentication requirements
- Removing endpoints
- Changing HTTP methods

### How to Handle Breaking Changes

1. **Minor changes**: Add new endpoint, deprecate old one
2. **Major redesign**: Create v2, maintain v1 for 6+ months
3. **Documentation**: Update migration guide

## Migration Strategy

### From Unversioned to v1

**Status:** Completed ✅

- Moved all routes under `/api/v1` prefix
- Removed duplicate route registrations
- Fixed non-RESTful endpoints (POST → PUT for favorites)

**Client Impact:**
- Old paths: `/search`, `/files` → **Removed**
- New paths: `/api/v1/search`, `/api/v1/files`

### Future v2 Migration (When Needed)

When v2 is needed:

1. Create `src/api/v2/` directory
2. Copy v1 routes as starting point
3. Make breaking changes
4. Update `src/api/app.py` to include both routers
5. Add deprecation headers to v1
6. Document migration in `/docs/MIGRATION_v1_to_v2.md`

## Version Support Policy

| Version | Status | Support Until | Notes |
|---------|--------|---------------|-------|
| v1 | **Active** | Ongoing | Current stable version |
| v2 | Planned | TBD | Will be created when breaking changes are required |

## Multiple Version Support

When multiple versions are active:

```python
# src/api/app.py
from src.api.v1 import router as api_v1_router
from src.api.v2 import router as api_v2_router

app.include_router(api_v1_router)
app.include_router(api_v2_router)
```

Each version is fully independent:
- Separate route handlers
- Separate schemas
- Separate service layer can be shared

## OpenAPI Documentation

Each version has its own OpenAPI spec:

- v1: `/api/v1/openapi.json`
- v1 docs: `/api/docs` (default points to latest)

To generate per-version docs:

```python
app_v1 = FastAPI(
    title="Vault API v1",
    version="1.0.0",
    openapi_url="/api/v1/openapi.json",
    docs_url="/api/v1/docs"
)

app_v2 = FastAPI(
    title="Vault API v2", 
    version="2.0.0",
    openapi_url="/api/v2/openapi.json",
    docs_url="/api/v2/docs"
)
```

## API Stability Guarantees

### What We Guarantee

- **Field presence**: Documented fields won't be removed
- **Data types**: Field types won't change
- **Semantics**: Endpoint behavior remains consistent
- **Status codes**: Success/error codes won't change for same scenarios

### What We Don't Guarantee

- **Performance**: Response times may vary
- **Internal implementation**: Backend changes transparently
- **Error details**: Error message formatting may improve
- **New fields**: Response objects may gain new optional fields

## Best Practices for API Consumers

### Future-Proof Your Client

```python
# ✅ Good: Ignore unknown fields
response = {"id": 1, "name": "test", "new_field": "value"}
data = MyModel(id=response["id"], name=response["name"])

# ❌ Bad: Strict parsing
data = MyModel(**response)  # Breaks when new fields added
```

### Check Deprecation Headers

```python
response = requests.get("/api/v1/endpoint")
if "Deprecation" in response.headers:
    logger.warning(f"Endpoint deprecated, sunset: {response.headers.get('Sunset')}")
    successor = response.headers.get("Link")
    # Plan migration to successor endpoint
```

### Version Pin in Production

```python
# ✅ Good: Explicit version
BASE_URL = "https://api.example.com/api/v1"

# ❌ Bad: Implicit latest
BASE_URL = "https://api.example.com/api"
```

## Change Log

### 2024-11-17: v1 Route Consolidation

**Changes:**
- Removed duplicate route registrations in `v1/__init__.py`
- Fixed `POST /documents/{id}/favorite` → `PUT /documents/{id}/favorite`
- Standardized all routes under `/api/v1` prefix
- Removed legacy non-versioned endpoints

**Migration:**
```
OLD: POST /api/v1/documents/{id}/favorite (toggle)
NEW: PUT /api/v1/documents/{id}/favorite (add)
     DELETE /api/v1/documents/{id}/favorite (remove)
```

---

## References

- [RFC 8594 - Sunset Header](https://datatracker.ietf.org/doc/html/rfc8594)
- [RFC 7231 - HTTP Semantics](https://datatracker.ietf.org/doc/html/rfc7231)
- [API Versioning Best Practices](https://www.ietf.org/archive/id/draft-ietf-httpapi-versioning-01.html)

## Contact

For questions about API versioning or to request changes:
- Open an issue in the repository
- Tag with `api` and `versioning` labels
