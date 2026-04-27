# Integrations Module

**Brick:** External Service Integrations
**Purpose:** Connect to and sync data from third-party services (Gmail, Slack, etc.)
**Contract:** Authenticate → Sync/Search → Standardized Data

## Overview

Self-contained module for integrating external services into the Recall knowledge base. Each integration authenticates users, syncs data, and returns standardized format.

## Public Interface

### Base Classes

#### `BaseIntegration`

Abstract base class for all integrations.

```python
class BaseIntegration(ABC):
    def __init__(self, config: IntegrationConfig)

    @property
    @abstractmethod
    def name(self) -> str

    @abstractmethod
    async def authenticate(self) -> bool

    @abstractmethod
    async def sync_data(
        self,
        since: Optional[datetime] = None,
        limit: Optional[int] = None
    ) -> list[IntegrationData]

    @abstractmethod
    async def search(
        self,
        query: str,
        limit: Optional[int] = None
    ) -> list[IntegrationData]

    async def disconnect(self) -> None
    async def get_status(self) -> IntegrationStatus
    async def healthcheck(self) -> bool
```

#### `IntegrationData`

Standardized data format returned by all integrations.

```python
class IntegrationData(BaseModel):
    id: str                      # Unique identifier from source
    content: str                 # Main text content
    metadata: dict[str, Any]     # Additional context
    timestamp: datetime          # Creation/modification time
    source: str                  # Integration name (e.g., "gmail")
```

#### `IntegrationConfig`

Base configuration for integrations.

```python
class IntegrationConfig(BaseModel):
    enabled: bool = True
    user_id: str
    credentials_path: Optional[str] = None
    last_sync: Optional[datetime] = None
    status: IntegrationStatus = DISCONNECTED
```

### Gmail Integration

#### `GmailIntegration`

Gmail email synchronization with OAuth 2.0.

```python
class GmailIntegration(BaseIntegration):
    def __init__(self, config: GmailConfig)

    async def authenticate(self) -> bool
    async def sync_data(
        since: Optional[datetime] = None,
        limit: Optional[int] = None
    ) -> list[IntegrationData]
    async def search(
        query: str,  # Gmail query syntax
        limit: Optional[int] = None
    ) -> list[IntegrationData]
```

#### `GmailConfig`

```python
class GmailConfig(IntegrationConfig):
    client_secrets_path: str      # OAuth client secrets JSON
    token_storage_path: str       # Token storage directory
    max_results: int = 100        # Max emails per batch (1-500)
    exclude_spam: bool = True     # Filter spam
    exclude_trash: bool = True    # Filter trash
    label_filters: list[str] = [] # Only sync specific labels
```

## Usage Examples

### Basic Gmail Sync

```python
from services.integrations import GmailIntegration, GmailConfig

# Configure
config = GmailConfig(
    user_id="user123",
    client_secrets_path="./secrets/gmail_client_secrets.json",
    token_storage_path="./storage/tokens"
)

# Initialize
gmail = GmailIntegration(config)

# Authenticate (OAuth 2.0 flow)
await gmail.authenticate()

# Sync recent emails
emails = await gmail.sync_data(limit=50)

# Search
results = await gmail.search("from:boss@company.com subject:urgent")

# Access standardized data
for email in emails:
    print(f"ID: {email.id}")
    print(f"Content: {email.content[:100]}...")
    print(f"Subject: {email.metadata['subject']}")
    print(f"From: {email.metadata['sender']}")
    print(f"Time: {email.timestamp}")
```

### Incremental Sync

```python
from datetime import datetime, timedelta

# Sync only emails from last 7 days
since = datetime.now() - timedelta(days=7)
recent_emails = await gmail.sync_data(since=since, limit=100)
```

### Advanced Configuration

```python
config = GmailConfig(
    user_id="user123",
    client_secrets_path="./secrets/gmail_client_secrets.json",
    token_storage_path="./storage/tokens",
    max_results=200,
    exclude_spam=True,
    exclude_trash=True,
    label_filters=["IMPORTANT", "CATEGORY_PERSONAL"]
)
```

## Data Format

### Input

**Gmail OAuth Credentials:**
- Format: JSON file from Google Cloud Console
- Contains: `client_id`, `client_secret`, `redirect_uris`
- Location: `gmail_client_secrets_path` setting

**Gmail Query Syntax:**
- `from:sender@example.com` - Filter by sender
- `subject:meeting` - Filter by subject
- `after:2024/01/01` - Date filtering
- `label:important` - Label filtering
- `-in:spam` - Exclude spam

### Output

```python
IntegrationData(
    id="msg_abc123",
    content="Email body text...",
    metadata={
        "subject": "Meeting Notes",
        "sender": "colleague@example.com",
        "to": "me@example.com",
        "cc": "team@example.com",
        "labels": ["INBOX", "IMPORTANT"],
        "thread_id": "thread_xyz789",
        "snippet": "Email preview text..."
    },
    timestamp=datetime(2024, 1, 15, 10, 30, 0),
    source="gmail"
)
```

## Side Effects

### File System
- **OAuth Tokens:** Written to `{token_storage_path}/{user_id}_token.json`
- **Format:** JSON with encrypted credentials
- **Permissions:** Read/write by application only

### Network
- **Gmail API:** HTTPS requests to `https://gmail.googleapis.com/gmail/v1/`
- **Rate Limits:** 250 quota units per user per second
- **OAuth:** `https://accounts.google.com/o/oauth2/auth`

### Logging
- Structured logs via `structlog`
- Events: authentication, sync, errors, rate limits

## Dependencies

| Package | Version | Purpose |
|---------|---------|---------|
| google-auth | ^2.25.2 | OAuth 2.0 authentication |
| google-auth-oauthlib | ^1.2.0 | OAuth flow handling |
| google-api-python-client | ^2.111.0 | Gmail API client |
| langchain-google-community | ^1.0.0 | Langchain Gmail toolkit |
| beautifulsoup4 | ^4.12.2 | HTML to text conversion |
| httpx | ^0.25.2 | Async HTTP client |
| structlog | ^23.2.0 | Structured logging |
| pydantic | ^2.5.2 | Data validation |

## Error Handling

### Exception Types

#### `AuthenticationError`
- **Condition:** OAuth flow fails or credentials invalid
- **Recovery:** Re-run OAuth flow, check client secrets
- **Example:**
  ```python
  try:
      await gmail.authenticate()
  except AuthenticationError as e:
      logger.error(f"Auth failed: {e}")
      # Prompt user to re-authorize
  ```

#### `RateLimitError`
- **Condition:** Gmail API rate limit exceeded
- **Recovery:** Automatic exponential backoff (5 retries)
- **Attributes:** `retry_after` (seconds)
- **Example:**
  ```python
  try:
      emails = await gmail.sync_data()
  except RateLimitError as e:
      logger.warning(f"Rate limited, retry after {e.retry_after}s")
      await asyncio.sleep(e.retry_after)
  ```

#### `SyncError`
- **Condition:** Network error, API error, parsing failure
- **Recovery:** Log error, skip item, continue sync
- **Example:**
  ```python
  try:
      emails = await gmail.sync_data()
  except SyncError as e:
      logger.error(f"Sync failed: {e}")
      # Retry with exponential backoff or alert user
  ```

### Error Response Table

| Error | HTTP Status | Retry? | Action |
|-------|-------------|--------|--------|
| Invalid credentials | 401 | No | Re-authenticate |
| Rate limit | 429 | Yes | Exponential backoff |
| Network timeout | 503 | Yes | Retry with backoff |
| Invalid query | 400 | No | Fix query syntax |
| Quota exceeded | 403 | No | Wait 24h or upgrade |

## Performance Characteristics

### Time Complexity
- `authenticate()`: O(1) - Single OAuth flow
- `sync_data(limit=N)`: O(N) - Linear with email count
- `search(query, limit=N)`: O(N) - Linear with results

### Memory Usage
- ~1KB per email metadata
- ~10KB per email with content
- 100 emails ≈ 1MB memory

### Rate Limits
- **Gmail API:** 250 quota units/user/second
- **Operations:**
  - `messages.list`: 5 units
  - `messages.get`: 5 units
- **Max throughput:** ~50 messages/second
- **Automatic retry:** Exponential backoff (5 attempts max)

### Concurrent Requests
- Sequential by default (Gmail API limitation)
- Batch requests not implemented (complex API)

## Configuration

### Environment Variables

```bash
# In .env or settings
GMAIL_CLIENT_SECRETS_PATH=./secrets/gmail_client_secrets.json
GMAIL_TOKEN_STORAGE_PATH=./storage/tokens
GMAIL_MAX_RESULTS=100
GMAIL_EXCLUDE_SPAM=true
GMAIL_EXCLUDE_TRASH=true
GMAIL_SYNC_INTERVAL=3600
```

### Settings.py

```python
gmail_client_secrets_path: str = "./secrets/gmail_client_secrets.json"
gmail_token_storage_path: str = "./storage/tokens"
gmail_max_results: int = 100
gmail_exclude_spam: bool = True
gmail_exclude_trash: bool = True
gmail_sync_interval: int = 3600  # seconds
```

## Testing

```bash
# Run unit tests
pytest src/services/integrations/tests/

# Run integration tests (requires credentials)
pytest src/services/integrations/tests/ -m integration

# Run with coverage
pytest src/services/integrations/tests/ --cov=src.services.integrations
```

## Security

### OAuth 2.0 Flow
1. User authorizes via Google consent screen
2. App receives authorization code
3. Exchanges code for access + refresh tokens
4. Tokens stored encrypted in `token_storage_path`
5. Refresh token used to get new access tokens

### Token Storage
- **Location:** `{token_storage_path}/{user_id}_token.json`
- **Permissions:** Application-only read/write
- **Contents:** Encrypted OAuth credentials
- **Rotation:** Automatic via refresh token

### Best Practices
- Never commit client secrets to git
- Use environment variables for paths
- Rotate client secrets periodically
- Monitor for suspicious activity
- Implement token revocation on account deletion

## Regeneration Specification

This module can be fully regenerated from this specification.

### Key Invariants
1. **Public Interface:** All abstract methods in `BaseIntegration`
2. **Data Format:** `IntegrationData` structure unchanged
3. **Error Types:** `AuthenticationError`, `RateLimitError`, `SyncError`
4. **OAuth Flow:** Standard OAuth 2.0 with refresh tokens
5. **Rate Limiting:** Exponential backoff with 5 retries

### Implementation Requirements
- OAuth 2.0 with `google-auth-oauthlib`
- Gmail API via `google-api-python-client`
- HTML to text via `beautifulsoup4`
- Async operations with `asyncio.to_thread` for sync libs
- Structured logging with `structlog`
- Type hints and Pydantic models

## Extension Points

### Adding New Integrations

```python
from .base import BaseIntegration, IntegrationConfig, IntegrationData

class SlackConfig(IntegrationConfig):
    bot_token: str
    channels: list[str]

class SlackIntegration(BaseIntegration):
    @property
    def name(self) -> str:
        return "slack"

    async def authenticate(self) -> bool:
        # Implement Slack OAuth
        pass

    async def sync_data(...) -> list[IntegrationData]:
        # Fetch messages, convert to IntegrationData
        pass

    async def search(...) -> list[IntegrationData]:
        # Search Slack, convert to IntegrationData
        pass
```

### Adding to `__init__.py`

```python
from .slack import SlackIntegration, SlackConfig

__all__ = [
    # ... existing exports
    "SlackIntegration",
    "SlackConfig",
]
```

## Architecture

```
integrations/
├── __init__.py          # Public exports
├── base.py              # Abstract base classes
├── gmail.py             # Gmail implementation
├── README.md            # This file
└── tests/
    ├── test_base.py
    ├── test_gmail.py
    └── fixtures/
        └── sample_email.json
```

## Status

- ✅ Base classes implemented
- ✅ Gmail integration complete
- ✅ OAuth 2.0 authentication
- ✅ Rate limiting with exponential backoff
- ✅ HTML to text conversion
- ✅ Structured logging
- ⏳ Slack integration (future)
- ⏳ Notion integration (future)
