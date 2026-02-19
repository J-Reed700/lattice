# Rate Limiting Documentation

## Overview

Vault implements comprehensive rate limiting to protect against brute force attacks, credential stuffing, denial of service attacks, and API abuse. Rate limiting is enforced using Redis as a distributed backend, ensuring consistent limits across multiple application instances.

## Security Benefits

### 1. Brute Force Protection
- **Login Attempts**: Limited to 5/minute per IP and 10/hour per username
- **Password Guessing**: Attackers cannot rapidly try passwords
- **Account Enumeration**: Slows down username/email discovery attempts

### 2. Credential Stuffing Prevention
- **Cross-Site Attacks**: Per-username limits prevent using leaked credentials from other breaches
- **Distributed Attacks**: Per-IP limits stop attacks from multiple IPs

### 3. Denial of Service (DoS) Prevention
- **Resource Exhaustion**: Prevents overwhelming the system with requests
- **Cost Control**: Limits expensive operations (OCR, LLM, embeddings)

### 4. API Abuse Prevention
- **Fair Usage**: Ensures all users get fair access to resources
- **Spam Prevention**: Registration limits prevent spam accounts

## Rate Limit Configuration

### Authentication Endpoints

| Endpoint | IP-based Limit | User-based Limit | Purpose |
|----------|----------------|------------------|---------|
| POST /auth/token | 5/minute | 10/hour per username | Prevent brute force login |
| POST /auth/register | 3/hour | N/A | Prevent spam registrations |

### File Operations

| Endpoint | Limit | Purpose |
|----------|-------|---------|
| POST /files/ | 10/minute | Prevent upload spam |
| GET /files/{id}/download | 100/minute | Allow reasonable downloads |
| DELETE /files/{id} | 20/minute | Prevent bulk deletion |

## Configuration

### Environment Variables

Configure rate limits via environment variables:

RATE_LIMITING_ENABLED=true
REDIS_URL=redis://localhost:6379/1
RATE_LIMIT_LOGIN_PER_MINUTE=5
RATE_LIMIT_LOGIN_PER_HOUR=10
RATE_LIMIT_REGISTER_PER_HOUR=3
RATE_LIMIT_FILE_UPLOAD_PER_MINUTE=10
RATE_LIMIT_SEARCH_PER_MINUTE=60

## Testing

Run security tests:
pytest tests/security/test_rate_limiting.py -v

## References

- OWASP Authentication Cheat Sheet
- NIST Digital Identity Guidelines
- slowapi Documentation
