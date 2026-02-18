# Authentication System

## Overview

This authentication system provides secure JWT-based authentication for the Vault API. It includes:

- User registration and login
- Password hashing with bcrypt
- JWT token generation and validation
- Protected endpoint dependencies
- OAuth2-compatible token endpoint

## Security Features

✅ **Password Security**:
- Bcrypt hashing with automatic salt
- Minimum 8 characters with complexity requirements
- Never stores plaintext passwords

✅ **Token Security**:
- JWT tokens with configurable expiration (default: 30 minutes)
- Signed with secret key from environment
- Includes user ID and username claims

✅ **API Protection**:
- OAuth2 Bearer token authentication
- Active user validation
- Optional superuser checks

## Quick Start

### 1. Enable Authentication on Endpoints

Add authentication to any endpoint by adding the dependency:

```python
from src.auth.dependencies import get_current_active_user
from src.auth.models import User

@router.post("/files")
async def upload_file(
    file: UploadFile = File(...),
    current_user: User = Depends(get_current_active_user)  # ✅ ADD THIS
):
    # Only authenticated, active users can access this
    # current_user contains the User object
    return {"uploaded_by": current_user.username}
```

### 2. Different Authentication Levels

**Active users only** (recommended for most endpoints):
```python
from src.auth.dependencies import get_current_active_user

@router.post("/documents")
async def create_document(
    user: User = Depends(get_current_active_user)
):
    pass
```

**Any authenticated user** (including inactive):
```python
from src.auth.dependencies import get_current_user

@router.get("/profile")
async def get_profile(
    user: User = Depends(get_current_user)
):
    pass
```

**Superuser only** (admin endpoints):
```python
from src.auth.dependencies import get_current_superuser

@router.delete("/users/{user_id}")
async def delete_user(
    user_id: int,
    admin: User = Depends(get_current_superuser)
):
    pass
```

**Optional authentication** (public endpoints with enhanced features when authenticated):
```python
from src.auth.dependencies import get_current_user_optional

@router.get("/search")
async def search(
    query: str,
    user: Optional[User] = Depends(get_current_user_optional)
):
    # Works without auth, but can personalize results if user is logged in
    if user:
        # Personalized search
        pass
    else:
        # Public search
        pass
```

## API Usage

### Register New User

```bash
POST /api/v1/auth/register
Content-Type: application/json

{
  "username": "john_doe",
  "email": "john@example.com",
  "password": "SecurePass123"
}
```

**Response**:
```json
{
  "id": 1,
  "username": "john_doe",
  "email": "john@example.com",
  "is_active": true,
  "is_superuser": false,
  "created_at": "2024-01-10T12:00:00"
}
```

### Login (Get Token)

```bash
POST /api/v1/auth/token
Content-Type: application/x-www-form-urlencoded

username=john_doe&password=SecurePass123
```

**Response**:
```json
{
  "access_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "token_type": "bearer",
  "expires_in": 1800
}
```

### Use Token in Requests

Include the token in the Authorization header:

```bash
GET /api/v1/files
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
```

### Get Current User Info

```bash
GET /api/v1/auth/me
Authorization: Bearer <token>
```

## Desktop App Integration

For the Tauri desktop app (local-first), authentication can be **disabled** by setting an environment variable:

```env
# In .env
ENABLE_AUTH=false  # Disable auth for local desktop use
```

When disabled, the API will work without tokens (suitable for single-user local deployments).

## Database Migration

Run migration to create users table:

```bash
# Create migration
alembic revision --autogenerate -m "Add users table for authentication"

# Apply migration
alembic upgrade head
```

## Configuration

Authentication settings in `settings.py`:

```python
jwt_secret_key: str = Field(..., min_length=32)  # MUST be set in .env
jwt_algorithm: str = "HS256"
jwt_access_token_expire_minutes: int = 30
```

## Testing

Test protected endpoint returns 401 without token:

```bash
curl -X GET http://localhost:8000/api/v1/files
# Expected: 401 Unauthorized
```

Test with valid token:

```bash
TOKEN=$(curl -X POST http://localhost:8000/api/v1/auth/token \
  -d "username=john_doe&password=SecurePass123" | jq -r '.access_token')

curl -X GET http://localhost:8000/api/v1/files \
  -H "Authorization: Bearer $TOKEN"
# Expected: 200 OK with file list
```

## Security Checklist

- [ ] SECRET_KEY is set to a secure random value (not default)
- [ ] SECRET_KEY is at least 32 characters
- [ ] SECRET_KEY is never committed to version control
- [ ] Token expiration is appropriate (30 minutes recommended)
- [ ] All write endpoints require authentication
- [ ] All sensitive read endpoints require authentication
- [ ] Admin endpoints require superuser check
- [ ] Password complexity requirements are enforced
- [ ] HTTPS is used in production (not HTTP)

## Common Issues

**401 Unauthorized**:
- Token expired (default: 30 minutes)
- Invalid token format
- Token not included in Authorization header
- User was deleted or deactivated

**403 Forbidden**:
- User account is inactive (`is_active=False`)
- User lacks required permissions (e.g., not superuser)

**400 Bad Request on registration**:
- Username or email already exists
- Password doesn't meet complexity requirements
