"""Tests for configuration validation."""

from pydantic import ValidationError
import pytest

from src.config.settings import Settings


@pytest.mark.unit()
class TestJWTSecretKeyValidation:
    """Test JWT secret key validation."""

    def test_valid_secret_key(self) -> None:
        """Test that a valid secret key is accepted."""
        valid_key = "a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6q7r8s9t0u1v2w3x4y5z6"
        settings = Settings(jwt_secret_key=valid_key)
        assert settings.jwt_secret_key == valid_key

    def test_secret_key_too_short(self) -> None:
        """Test that a secret key shorter than 32 characters is rejected."""
        with pytest.raises(ValidationError) as exc_info:
            Settings(jwt_secret_key="short")

        errors = exc_info.value.errors()
        assert len(errors) == 1
        assert "at least 32 characters" in errors[0]["msg"]

    def test_secret_key_weak_pattern_your_secret_key(self) -> None:
        """Test that weak pattern 'your-secret-key' is rejected."""
        with pytest.raises(ValidationError) as exc_info:
            Settings(jwt_secret_key="your-secret-key-change-in-production")

        errors = exc_info.value.errors()
        assert len(errors) == 1
        assert "weak pattern" in errors[0]["msg"].lower()
        assert "your-secret-key" in errors[0]["msg"].lower()

    def test_secret_key_weak_pattern_change_in_production(self) -> None:
        """Test that weak pattern 'change-in-production' is rejected."""
        with pytest.raises(ValidationError) as exc_info:
            Settings(jwt_secret_key="please-change-in-production-12345678")

        errors = exc_info.value.errors()
        assert len(errors) == 1
        assert "weak pattern" in errors[0]["msg"].lower()
        assert "change-in-production" in errors[0]["msg"].lower()

    def test_secret_key_weak_pattern_replace_with(self) -> None:
        """Test that weak pattern 'REPLACE_WITH' is rejected."""
        with pytest.raises(ValidationError) as exc_info:
            Settings(jwt_secret_key="REPLACE_WITH_YOUR_SECRET_KEY_12345678")

        errors = exc_info.value.errors()
        assert len(errors) == 1
        assert "weak pattern" in errors[0]["msg"].lower()
        assert "replace_with" in errors[0]["msg"].lower()

    def test_secret_key_weak_pattern_test_secret(self) -> None:
        """Test that weak pattern 'test-secret' is rejected."""
        with pytest.raises(ValidationError) as exc_info:
            Settings(jwt_secret_key="test-secret-key-for-development-12345")

        errors = exc_info.value.errors()
        assert len(errors) == 1
        assert "weak pattern" in errors[0]["msg"].lower()
        assert "test-secret" in errors[0]["msg"].lower()

    def test_secret_key_weak_pattern_development_key(self) -> None:
        """Test that weak pattern 'development-key' is rejected."""
        with pytest.raises(ValidationError) as exc_info:
            Settings(jwt_secret_key="development-key-not-for-production-123")

        errors = exc_info.value.errors()
        assert len(errors) == 1
        assert "weak pattern" in errors[0]["msg"].lower()
        assert "development-key" in errors[0]["msg"].lower()

    def test_secret_key_insufficient_entropy_repeated_char(self) -> None:
        """Test that a secret key with repeated characters is rejected."""
        # 32 'a' characters - only 1 unique character
        with pytest.raises(ValidationError) as exc_info:
            Settings(jwt_secret_key="a" * 32)

        errors = exc_info.value.errors()
        assert len(errors) == 1
        assert "insufficient entropy" in errors[0]["msg"].lower()

    def test_secret_key_insufficient_entropy_few_unique_chars(self) -> None:
        """Test that a secret key with too few unique characters is rejected."""
        # Only 10 unique characters (aaabbbcccddd...)
        with pytest.raises(ValidationError) as exc_info:
            Settings(jwt_secret_key="abcdefghij" * 4)

        errors = exc_info.value.errors()
        assert len(errors) == 1
        assert "insufficient entropy" in errors[0]["msg"].lower()

    def test_secret_key_good_entropy(self) -> None:
        """Test that a secret key with good entropy is accepted."""
        # 26 unique characters (alphabet)
        good_key = "abcdefghijklmnopqrstuvwxyz123456"
        settings = Settings(jwt_secret_key=good_key)
        assert settings.jwt_secret_key == good_key

    def test_secret_key_case_insensitive_pattern_matching(self) -> None:
        """Test that weak pattern matching is case-insensitive."""
        with pytest.raises(ValidationError) as exc_info:
            Settings(jwt_secret_key="YOUR-SECRET-KEY-CHANGE-IN-PRODUCTION")

        errors = exc_info.value.errors()
        assert len(errors) == 1
        assert "weak pattern" in errors[0]["msg"].lower()

    def test_secret_key_error_message_includes_generation_hint(self) -> None:
        """Test that error message includes hint on generating secure key."""
        with pytest.raises(ValidationError) as exc_info:
            Settings(jwt_secret_key="your-secret-key-12345678901234567890")

        errors = exc_info.value.errors()
        assert len(errors) == 1
        assert "python -c 'import secrets" in errors[0]["msg"]
        assert "token_urlsafe" in errors[0]["msg"]

    def test_secret_key_exactly_32_chars_valid(self) -> None:
        """Test that a 32 character key with good entropy is accepted."""
        key_32 = "abcdefghijklmnopqrstuvwxyz123456"
        assert len(key_32) == 32
        settings = Settings(jwt_secret_key=key_32)
        assert settings.jwt_secret_key == key_32

    def test_secret_key_minimum_16_unique_chars(self) -> None:
        """Test that exactly 16 unique characters is sufficient entropy."""
        # 16 unique characters repeated twice (32 total)
        key = "abcdefghijklmnop" * 2
        assert len(set(key)) == 16
        settings = Settings(jwt_secret_key=key)
        assert settings.jwt_secret_key == key

    def test_secret_key_15_unique_chars_rejected(self) -> None:
        """Test that 15 unique characters is insufficient entropy."""
        # 15 unique characters
        key = "abcdefghijklmnoabcdefghijklmno12"
        assert len(set(key)) == 15
        with pytest.raises(ValidationError) as exc_info:
            Settings(jwt_secret_key=key)

        errors = exc_info.value.errors()
        assert len(errors) == 1
        assert "insufficient entropy" in errors[0]["msg"].lower()
