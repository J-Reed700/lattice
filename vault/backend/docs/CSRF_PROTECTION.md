# CSRF Protection Implementation Guide

## Overview

This document explains the CSRF (Cross-Site Request Forgery) protection implementation for the Vault API.

## What is CSRF?

CSRF is an attack that tricks authenticated users into making unwanted requests to a web application. 

**Example Attack:**
```html
<!-- Attacker website -->
<img src="https://vault.example.com/api/v1/files/123" 
     onload="fetch('" + "'https://vault.example.com/api/v1/files/123', {method: 'DELETE'})">
```

If the user is logged in, their file gets deleted without their knowledge.

## Implementation Architecture

### Double Submit Cookie Pattern

We use the stateless **Double Submit Cookie** pattern:

1. Server generates cryptographically secure CSRF token
2. Token sent in cookie AND must be included in request header
3. Server validates both tokens match on state-changing requests
4. Safe methods (GET, HEAD, OPTIONS) bypass CSRF check

**Why This Works:**
- Attacker websites cannot read cookies from other domains (Same-Origin Policy)
- Even if attacker can trigger a request, they cannot read the cookie to include in header

## Files Created

### Core Implementation

1. **src/middleware/csrf.py** - CSRF middleware and protection logic
   - `CSRFMiddleware` - ASGI middleware that sets CSRF cookie
   - `csrf_protect()` - FastAPI dependency for endpoint protection
   - `generate_csrf_token()` - Cryptographically secure token generation
   - `verify_csrf_token()` - Constant-time token comparison

2. **src/api/v1/csrf.py** - CSRF token endpoint
   - `GET /api/v1/csrf/token` - Get CSRF token for current session

3. **src/config/settings.py** - CSRF configuration
   - `csrf_enabled` - Enable/disable CSRF protection
   - `csrf_cookie_secure` - HTTPS-only cookie transmission
   - `csrf_cookie_samesite` - SameSite attribute (Strict/Lax/None)

4. **src/api/app.py** - Integration into FastAPI app
   - CSRF middleware added to middleware stack

### Testing & Documentation

5. **tests/security/test_csrf.py** - Comprehensive test suite
6. **docs/CSRF_PROTECTION.md** - This documentation

## Configuration

Environment variables for CSRF protection:

```env
# Enable/disable CSRF protection (default: true)
CSRF_ENABLED=true

# CSRF token length in bytes (default: 32 = 64 hex chars)
CSRF_TOKEN_LENGTH=32

# Only send cookie over HTTPS (production: true)
CSRF_COOKIE_SECURE=true

# SameSite cookie attribute (Strict, Lax, or None)
CSRF_COOKIE_SAMESITE=Strict

# Make cookie HttpOnly (default: false - client needs to read token)
CSRF_COOKIE_HTTPONLY=false

# Custom cookie domain (optional)
CSRF_COOKIE_DOMAIN=

# Exempt paths (default: auth endpoints)
CSRF_EXEMPT_PATHS=/api/v1/auth/token,/api/v1/auth/register
```

## How to Apply CSRF Protection to Endpoints

### Step 1: Import CSRF Protection

```python
from src.middleware.csrf import csrf_protect
```

### Step 2: Add Dependency to Endpoint

```python
@router.post("/files")
async def upload_file(
    file: UploadFile = File(...),
    db: AsyncSession = Depends(get_db),
    csrf: None = Depends(csrf_protect),  # ADD THIS
) -> FileUploadResponse:
    """Upload file with CSRF protection"""
    pass
```

### Complete Example

**Before:**
```python
from fastapi import APIRouter, Depends
from sqlalchemy.ext.asyncio import AsyncSession

router = APIRouter(prefix="/files")

@router.post("/")
async def upload_file(
    db: AsyncSession = Depends(get_db)
):
    pass
```

**After:**
```python
from fastapi import APIRouter, Depends
from sqlalchemy.ext.asyncio import AsyncSession
from src.middleware.csrf import csrf_protect  # ADD IMPORT

router = APIRouter(prefix="/files")

@router.post("/")
async def upload_file(
    db: AsyncSession = Depends(get_db),
    csrf: None = Depends(csrf_protect)  # ADD DEPENDENCY
):
    pass
```

## Frontend Integration

### TypeScript/React Example

```typescript
// lib/csrf.ts
let csrfToken: string | null = null;

export async function getCsrfToken(): Promise<string> {
  if (csrfToken) return csrfToken;
  
  const response = await fetch("/api/v1/csrf/token", {
    credentials: "include"
  });
  
  const data = await response.json();
  csrfToken = data.csrf_token;
  return csrfToken;
}

// lib/api.ts
import { getCsrfToken } from "./csrf";

export async function apiRequest(url: string, options: RequestInit = {}) {
  const headers = new Headers(options.headers);
  
  if (["POST", "PUT", "DELETE", "PATCH"].includes(options.method || "GET")) {
    const token = await getCsrfToken();
    headers.set("X-CSRF-Token", token);
  }
  
  return fetch(url, {
    ...options,
    headers,
    credentials: "include"
  });
}
```

## Testing

### Run Tests

```bash
cd vault/backend
pytest tests/security/test_csrf.py -v
```

### Manual Testing

```bash
# Get CSRF token
curl -c cookies.txt http://localhost:8000/api/v1/csrf/token

# Try without CSRF token (should fail)
curl -b cookies.txt -X POST http://localhost:8000/api/v1/files/

# Try with CSRF token (should work)
TOKEN="<token_from_response>"
curl -b cookies.txt -X POST   -H "X-CSRF-Token: $TOKEN"   http://localhost:8000/api/v1/files/
```

## Security Best Practices

1. **Always use HTTPS in production** - Set `CSRF_COOKIE_SECURE=true`
2. **Use SameSite=Strict** - Maximum protection against cross-site requests
3. **Validate token length** - Use at least 32 bytes (64 hex chars)
4. **Dont log CSRF tokens** - They are secrets
5. **Exempt only necessary endpoints** - Auth endpoints that use credentials

## Exempt Endpoints

These endpoints do NOT require CSRF tokens:

- `/api/v1/auth/register` - Uses password as proof of intent
- `/api/v1/auth/token` - Uses credentials in request body
- `/api/v1/health` - Read-only health check
- `/api/docs`, `/api/redoc` - Documentation

## Troubleshooting

### "CSRF token missing in cookie"

**Cause:** Client did not call `/api/v1/csrf/token` first
**Solution:** Call CSRF endpoint on app initialization

### "CSRF token required in header"

**Cause:** Client did not include `X-CSRF-Token` header
**Solution:** Add header to POST/PUT/PATCH/DELETE requests

### "CSRF token validation failed"

**Cause:** Token mismatch between cookie and header
**Solution:** Ensure using same token from cookie

## References

- [OWASP CSRF Prevention](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html)
- [Double Submit Cookie Pattern](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html#double-submit-cookie)
- [SameSite Cookies](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Set-Cookie/SameSite)
