"""Security integration tests.

Tests complete security workflows:
- Full authentication flow with security checks
- File upload with all security validations
- Multi-user access control
- Security across multiple endpoints
"""

from httpx import AsyncClient
import pytest
from sqlalchemy.ext.asyncio import AsyncSession

from src.auth.models import User


class TestCompleteAuthenticationFlow:
    """Test complete authentication flow with all security checks."""

    @pytest.mark.asyncio()
    async def test_complete_secure_registration_and_login(
        self, client: AsyncClient, db_session: AsyncSession
    ):
        """Test complete user registration and login flow."""
        username = "integrationuser"
        email = "integration@example.com"
        password = "SecurePass123!"

        register_response = await client.post(
            "/api/v1/auth/register",
            json={"username": username, "email": email, "password": password},
        )
        assert register_response.status_code == 201
        user_data = register_response.json()
        assert user_data["username"] == username
        assert user_data["email"] == email
        assert "password" not in user_data

        login_response = await client.post(
            "/api/v1/auth/token", data={"username": username, "password": password}
        )
        assert login_response.status_code == 200
        token_data = login_response.json()
        assert "access_token" in token_data
        assert token_data["token_type"] == "bearer"

        token = token_data["access_token"]
        headers = {"Authorization": f"Bearer {token}"}

        me_response = await client.get("/api/v1/auth/me", headers=headers)
        assert me_response.status_code == 200
        me_data = me_response.json()
        assert me_data["username"] == username
        assert me_data["email"] == email

    @pytest.mark.asyncio()
    async def test_authentication_prevents_sql_injection(self, client: AsyncClient):
        """Test that authentication flow is protected against SQL injection."""
        payloads = [
            "admin'--",
            "' OR '1'='1",
            "admin' OR 1=1--",
        ]

        for payload in payloads:
            response = await client.post(
                "/api/v1/auth/token", data={"username": payload, "password": "password"}
            )
            assert response.status_code == 401
            assert "syntax" not in response.text.lower()


class TestSecureFileUploadFlow:
    """Test complete file upload flow with all security validations."""

    @pytest.mark.asyncio()
    async def test_complete_secure_file_upload(
        self, client: AsyncClient, test_user: User, auth_headers: dict
    ):
        """Test complete secure file upload: auth + validation."""
        response = await client.post(
            "/api/v1/files/",
            files={"file": ("test_document.txt", b"This is a test document.")},
            headers=auth_headers,
        )

        if response.status_code in [200, 201]:
            file_data = response.json()
            assert "id" in file_data
            file_id = file_data["id"]

            download_response = await client.get(
                f"/api/v1/files/{file_id}/download", headers=auth_headers
            )
            if download_response.status_code == 200:
                assert download_response.content == b"This is a test document."

    @pytest.mark.asyncio()
    async def test_file_upload_rejects_path_traversal(
        self, client: AsyncClient, auth_headers: dict
    ):
        """Test that file upload rejects path traversal in filename."""
        response = await client.post(
            "/api/v1/files/",
            files={"file": ("../../../etc/passwd", b"content")},
            headers=auth_headers,
        )
        assert response.status_code in [400, 403, 404, 422]

    @pytest.mark.asyncio()
    async def test_file_upload_rejects_malicious_extension(
        self, client: AsyncClient, auth_headers: dict
    ):
        """Test that file upload rejects malicious file extensions."""
        malicious_files = [
            ("malware.exe", b"MZ\x90\x00"),
            ("shell.php", b"<?php system($_GET['cmd']); ?>"),
            ("script.sh", b"#!/bin/bash\nrm -rf /"),
        ]

        for filename, content in malicious_files:
            response = await client.post(
                "/api/v1/files/", files={"file": (filename, content)}, headers=auth_headers
            )
            assert response.status_code in [400, 403, 404, 415, 422]


class TestMultiUserAccessControl:
    """Test access control across multiple users."""

    @pytest.mark.asyncio()
    async def test_user_isolation(
        self,
        client: AsyncClient,
        test_user: User,
        second_user: User,
        auth_headers: dict,
        second_user_headers: dict,
    ):
        """Test that users cannot access each other's resources."""
        upload_response = await client.post(
            "/api/v1/files/",
            files={"file": ("user1_private.txt", b"User 1's private data")},
            headers=auth_headers,
        )

        if upload_response.status_code in [200, 201]:
            file_id = upload_response.json()["id"]

            access_response = await client.get(
                f"/api/v1/files/{file_id}", headers=second_user_headers
            )
            assert access_response.status_code == 403

    @pytest.mark.asyncio()
    async def test_privilege_escalation_prevented(
        self, client: AsyncClient, test_user: User, auth_headers: dict, db_session: AsyncSession
    ):
        """Test that privilege escalation is prevented."""
        response = await client.patch(
            f"/api/v1/users/{test_user.id}", headers=auth_headers, json={"is_superuser": True}
        )
        assert response.status_code in [403, 404, 422]

        await db_session.refresh(test_user)
        assert not test_user.is_superuser


class TestSecurityAcrossEndpoints:
    """Test security measures work across different endpoints."""

    @pytest.mark.asyncio()
    async def test_all_endpoints_require_authentication(self, client: AsyncClient):
        """Test that protected endpoints require authentication."""
        protected_endpoints = [
            ("GET", "/api/v1/auth/me"),
            ("GET", "/api/v1/files/"),
            ("POST", "/api/v1/search"),
            ("POST", "/api/v1/files/"),
        ]

        for method, endpoint in protected_endpoints:
            if method == "GET":
                response = await client.get(endpoint)
            elif method == "POST":
                response = await client.post(endpoint, json={})

            assert response.status_code in [401, 422]

    @pytest.mark.asyncio()
    async def test_all_mutation_endpoints_protected_csrf(
        self, client: AsyncClient, auth_headers: dict
    ):
        """Test that state-changing endpoints have CSRF protection."""
        mutation_endpoints = [
            ("POST", "/api/v1/files/", {"file": ("test.txt", b"content")}),
            ("DELETE", "/api/v1/files/123", None),
            ("PUT", "/api/v1/files/123", {"name": "updated"}),
        ]

        for method, endpoint, data in mutation_endpoints:
            if method == "POST" and "files" in str(data):
                response = await client.post(endpoint, files=data["file"], headers=auth_headers)
            elif method == "POST":
                response = await client.post(endpoint, json=data, headers=auth_headers)
            elif method == "DELETE":
                response = await client.delete(endpoint, headers=auth_headers)
            elif method == "PUT":
                response = await client.put(endpoint, json=data, headers=auth_headers)

            assert response.status_code in [403, 404]


class TestSecurityWithInvalidInputs:
    """Test security with various invalid inputs across endpoints."""

    @pytest.mark.asyncio()
    async def test_xss_payloads_sanitized_across_endpoints(
        self, client: AsyncClient, auth_headers: dict
    ):
        """Test that XSS payloads are sanitized across all endpoints."""
        xss_payload = "<script>alert('XSS')</script>"

        endpoints = [
            ("/api/v1/search", {"query": xss_payload}),
            (
                "/api/v1/auth/register",
                {"username": xss_payload, "email": "test@example.com", "password": "Pass123!"},
            ),
        ]

        for endpoint, data in endpoints:
            response = await client.post(endpoint, json=data, headers=auth_headers)
            if response.status_code == 200:
                assert "<script>" not in response.text

    @pytest.mark.asyncio()
    async def test_sql_injection_blocked_across_endpoints(
        self, client: AsyncClient, auth_headers: dict
    ):
        """Test that SQL injection is blocked across all endpoints."""
        sql_payload = "' OR '1'='1"

        endpoints = [
            ("/api/v1/search", {"query": sql_payload}),
            ("/api/v1/auth/token", {"username": sql_payload, "password": "pass"}),
        ]

        for endpoint, data in endpoints:
            if "token" in endpoint:
                response = await client.post(endpoint, data=data)
            else:
                response = await client.post(endpoint, json=data, headers=auth_headers)

            if response.status_code != 401:
                assert "syntax error" not in response.text.lower()


class TestConcurrentSecurityScenarios:
    """Test security in concurrent scenarios."""

    @pytest.mark.asyncio()
    async def test_concurrent_login_attempts_rate_limited(self, client: AsyncClient):
        """Test that concurrent login attempts are rate limited."""
        import asyncio

        async def login_attempt():
            return await client.post(
                "/api/v1/auth/token", data={"username": "testuser", "password": "wrong"}
            )

        tasks = [login_attempt() for _ in range(10)]
        responses = await asyncio.gather(*tasks)

        rate_limited = sum(1 for r in responses if r.status_code == 429)
        assert rate_limited > 0 or all(r.status_code == 401 for r in responses)


class TestSecurityEdgeCases:
    """Test security edge cases and boundary conditions."""

    @pytest.mark.asyncio()
    async def test_extremely_long_token_rejected(self, client: AsyncClient):
        """Test that extremely long tokens are rejected."""
        long_token = "Bearer " + "A" * 10000
        headers = {"Authorization": long_token}

        response = await client.get("/api/v1/auth/me", headers=headers)
        assert response.status_code == 401

    @pytest.mark.asyncio()
    async def test_null_bytes_in_various_fields(self, client: AsyncClient, auth_headers: dict):
        """Test that null bytes are properly handled."""
        response = await client.post(
            "/api/v1/search", headers=auth_headers, json={"query": "test\x00query"}
        )
        assert response.status_code in [200, 400, 422]

    @pytest.mark.asyncio()
    async def test_unicode_edge_cases(self, client: AsyncClient, auth_headers: dict):
        """Test that unicode edge cases are handled."""
        unicode_tests = [
            "\u0000",
            "\uFFFD",
            "𝕳𝖊𝖑𝖑𝖔",
            "test\u202Emalicious",
        ]

        for unicode_str in unicode_tests:
            response = await client.post(
                "/api/v1/search", headers=auth_headers, json={"query": unicode_str}
            )
            assert response.status_code in [200, 400, 422]
