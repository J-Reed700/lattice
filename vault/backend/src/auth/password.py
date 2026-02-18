"""Password hashing and verification utilities."""

from passlib.context import CryptContext

# Configure bcrypt for password hashing
# Bcrypt is industry-standard for password storage
pwd_context = CryptContext(schemes=["bcrypt"], deprecated="auto")


def hash_password(password: str) -> str:
    """
    Hash a password using bcrypt.

    Args:
        password: Plain text password to hash

    Returns:
        Bcrypt hashed password string

    Example:
        >>> hashed = hash_password("MySecurePass123")
        >>> # Returns: "$2b$12$..."
    """
    return pwd_context.hash(password)


def verify_password(plain_password: str, hashed_password: str) -> bool:
    """
    Verify a password against its hash.

    Args:
        plain_password: Plain text password to verify
        hashed_password: Bcrypt hash to verify against

    Returns:
        True if password matches, False otherwise

    Example:
        >>> hashed = hash_password("MySecurePass123")
        >>> verify_password("MySecurePass123", hashed)  # True
        >>> verify_password("WrongPassword", hashed)  # False
    """
    return pwd_context.verify(plain_password, hashed_password)
