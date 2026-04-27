"""Path traversal security tests.

Tests protection against directory traversal attacks:
- Basic path traversal (../)
- URL encoded traversal
- Double encoded traversal
- Unicode encoding
- Null byte injection
- Windows UNC paths
- Symlink attacks
- Absolute paths
"""

from pathlib import Path

from httpx import AsyncClient
import pytest

from src.utils.security import SecurityError, validate_filename, validate_safe_path


class TestPathTraversalInAPI:
    """Test path traversal protection in API endpoints."""

    @pytest.mark.asyncio()
    @pytest.mark.parametrize(
        "payload",
        [
            "../../../etc/passwd",
            "..\\..\\..\\windows\\system32\\config\\sam",
            "....//....//....//etc/passwd",
            "..%2F..%2F..%2Fetc%2Fpasswd",
            "..%252F..%252F..%252Fetc%252Fpasswd",
            "%2e%2e%2f%2e%2e%2f%2e%2e%2fetc%2fpasswd",
            "..%00/etc/passwd",
            "....\\\\....\\\\....\\\\windows\\\\win.ini",
            "/etc/passwd",
            "file:///etc/passwd",
            "../../../../../../../../../../etc/passwd",
        ],
    )
    async def test_path_traversal_in_file_download(
        self, client: AsyncClient, auth_headers: dict, payload: str
    ):
        """Test that path traversal is blocked in file download."""
        response = await client.get(
            "/api/v1/files/download", params={"path": payload}, headers=auth_headers
        )
        assert response.status_code in [400, 403, 404, 422]
        assert response.status_code != 200

    @pytest.mark.asyncio()
    @pytest.mark.parametrize(
        "payload",
        [
            "../../../etc/passwd",
            "..\\..\\..\\windows\\system32\\config\\sam",
            "/etc/passwd",
            "~/sensitive_file",
        ],
    )
    async def test_path_traversal_in_export(
        self, client: AsyncClient, auth_headers: dict, payload: str
    ):
        """Test that path traversal is blocked in export."""
        response = await client.post(
            "/api/v1/export/full", json={"output_path": payload}, headers=auth_headers
        )
        assert response.status_code in [400, 403, 404, 422]

    @pytest.mark.asyncio()
    async def test_absolute_path_blocked(self, client: AsyncClient, auth_headers: dict):
        """Test that absolute paths outside allowed directory are blocked."""
        response = await client.get(
            "/api/v1/files/download", params={"path": "/etc/passwd"}, headers=auth_headers
        )
        assert response.status_code in [400, 403, 404]


class TestValidateSafePath:
    """Test suite for validate_safe_path utility function."""

    def test_valid_relative_path(self, tmp_path: Path):
        """Test that valid relative paths are allowed."""
        base = tmp_path
        file_path = "documents/file.txt"

        result = validate_safe_path(file_path, base)
        assert result == base / "documents/file.txt"

    def test_valid_absolute_path_within_base(self, tmp_path: Path):
        """Test that valid absolute paths within base are allowed."""
        base = tmp_path
        file_path = base / "documents" / "file.txt"

        result = validate_safe_path(file_path, base)
        assert result == file_path.resolve()

    def test_path_traversal_blocked(self, tmp_path: Path):
        """Test that path traversal attempts are blocked."""
        base = tmp_path / "storage"
        base.mkdir()

        with pytest.raises(SecurityError, match="Path traversal detected"):
            validate_safe_path("../../etc/passwd", base)

    @pytest.mark.parametrize(
        "dangerous_path",
        [
            "../../../etc/passwd",
            "docs/../../outside.txt",
            "docs/../../../etc/shadow",
            "..\\..\\..\\windows\\system32\\config\\sam",
        ],
    )
    def test_complex_path_traversal_patterns(self, tmp_path: Path, dangerous_path: str):
        """Test complex path traversal patterns are blocked."""
        base = tmp_path / "storage"
        base.mkdir()

        with pytest.raises(SecurityError, match="Path traversal"):
            validate_safe_path(dangerous_path, base)

    def test_absolute_path_outside_base_blocked(self, tmp_path: Path):
        """Test that absolute paths outside base are blocked."""
        base = tmp_path / "storage"
        base.mkdir()

        outside = tmp_path / "outside" / "file.txt"

        with pytest.raises(SecurityError, match="Path traversal"):
            validate_safe_path(str(outside), base)

    def test_must_exist_validation(self, tmp_path: Path):
        """Test must_exist parameter."""
        base = tmp_path / "storage"
        base.mkdir()

        with pytest.raises(SecurityError, match="does not exist"):
            validate_safe_path("nonexistent.txt", base, must_exist=True)

        result = validate_safe_path("nonexistent.txt", base, must_exist=False)
        assert result == base / "nonexistent.txt"

    def test_symlink_blocked_by_default(self, tmp_path: Path):
        """Test that symbolic links are blocked by default."""
        base = tmp_path / "storage"
        base.mkdir()

        outside = tmp_path / "outside"
        outside.mkdir()
        outside_file = outside / "secret.txt"
        outside_file.write_text("secret data")

        link_path = base / "link"
        try:
            link_path.symlink_to(outside)
        except OSError:
            pytest.skip("Cannot create symlinks on this system")

        with pytest.raises(SecurityError, match="Symbolic links not allowed"):
            validate_safe_path(link_path, base, allow_symlinks=False)

    def test_symlink_allowed_when_permitted(self, tmp_path: Path):
        """Test that symlinks can be allowed explicitly."""
        base = tmp_path / "storage"
        base.mkdir()

        target = base / "target.txt"
        target.write_text("data")

        link = base / "link.txt"
        try:
            link.symlink_to(target)
        except OSError:
            pytest.skip("Cannot create symlinks on this system")

        result = validate_safe_path(link, base, allow_symlinks=True)
        assert result.resolve() == target.resolve()


class TestValidateFilename:
    """Test suite for validate_filename utility function."""

    @pytest.mark.parametrize(
        "valid_name",
        [
            "document.pdf",
            "image_2024.jpg",
            "my-file.txt",
            "file (1).doc",
            "report.xlsx",
        ],
    )
    def test_valid_filenames(self, valid_name: str):
        """Test that valid filenames are accepted."""
        assert validate_filename(valid_name) == valid_name

    @pytest.mark.parametrize(
        "dangerous_name",
        [
            "../etc/passwd",
            "..\\windows\\system32",
            "../../file.txt",
            "..\\..\\file.txt",
        ],
    )
    def test_path_traversal_in_filename_blocked(self, dangerous_name: str):
        """Test that filenames with path traversal are rejected."""
        with pytest.raises(SecurityError, match="path traversal"):
            validate_filename(dangerous_name)

    def test_null_byte_rejected(self):
        """Test that null bytes are rejected."""
        with pytest.raises(SecurityError, match="null byte"):
            validate_filename("file\x00.txt")

    @pytest.mark.parametrize(
        "filename",
        [
            "path/to/file.txt",
            "path\\to\\file.txt",
        ],
    )
    def test_path_separators_rejected_by_default(self, filename: str):
        """Test that path separators are rejected by default."""
        with pytest.raises(SecurityError, match="path separator"):
            validate_filename(filename)

    def test_path_separators_allowed_when_permitted(self):
        """Test that path separators can be allowed."""
        result = validate_filename("path/to/file.txt", allow_path_separators=True)
        assert result == "path/to/file.txt"

    @pytest.mark.parametrize(
        "reserved_name",
        [
            "CON.txt",
            "PRN.doc",
            "AUX.pdf",
            "NUL.txt",
            "COM1.txt",
            "LPT1.doc",
        ],
    )
    def test_windows_reserved_names_rejected(self, reserved_name: str):
        """Test that Windows reserved names are rejected."""
        with pytest.raises(SecurityError, match="reserved Windows name"):
            validate_filename(reserved_name)

    @pytest.mark.parametrize("invalid_name", ["", "   ", "\t\n"])
    def test_empty_or_whitespace_rejected(self, invalid_name: str):
        """Test that empty or whitespace-only names are rejected."""
        with pytest.raises(SecurityError, match="Invalid filename"):
            validate_filename(invalid_name)

    def test_tilde_home_directory_blocked(self):
        """Test that tilde (~) is blocked to prevent home directory access."""
        with pytest.raises(SecurityError, match="path traversal"):
            validate_filename("~/secret.txt")


class TestSymlinkAttacks:
    """Test protection against symlink-based attacks."""

    def test_symlink_to_etc_passwd_blocked(self, tmp_path: Path):
        """Test that symlinks to system files are blocked."""
        base = tmp_path / "storage"
        base.mkdir()

        link_path = base / "passwd_link"
        try:
            link_path.symlink_to("/etc/passwd")
        except OSError:
            pytest.skip("Cannot create symlinks on this system")

        with pytest.raises(SecurityError):
            validate_safe_path(link_path, base, allow_symlinks=False, must_exist=True)

    def test_symlink_chain_blocked(self, tmp_path: Path):
        """Test that symlink chains are properly validated."""
        base = tmp_path / "storage"
        base.mkdir()

        outside = tmp_path / "outside"
        outside.mkdir()
        secret = outside / "secret.txt"
        secret.write_text("secret")

        link1 = base / "link1"
        link2 = base / "link2"

        try:
            link1.symlink_to(outside)
            link2.symlink_to(link1)
        except OSError:
            pytest.skip("Cannot create symlinks on this system")

        with pytest.raises(SecurityError):
            validate_safe_path(link2, base, allow_symlinks=False)


class TestWindowsSpecificAttacks:
    """Test Windows-specific path attack vectors."""

    @pytest.mark.parametrize(
        "unc_path",
        [
            "\\\\localhost\\c$\\windows\\system32\\config\\sam",
            "\\\\server\\share\\secret.txt",
            "//server/share/file.txt",
        ],
    )
    def test_unc_paths_blocked(self, tmp_path: Path, unc_path: str):
        """Test that UNC paths are blocked."""
        base = tmp_path / "storage"
        base.mkdir()

        with pytest.raises(SecurityError):
            validate_safe_path(unc_path, base)

    @pytest.mark.parametrize("reserved", ["CON", "PRN", "AUX", "NUL"])
    def test_reserved_device_names_blocked(self, reserved: str):
        """Test that Windows reserved device names are blocked."""
        with pytest.raises(SecurityError, match="reserved Windows name"):
            validate_filename(f"{reserved}.txt")


class TestURLEncodedTraversal:
    """Test protection against URL-encoded path traversal."""

    @pytest.mark.asyncio()
    @pytest.mark.parametrize(
        "encoded_payload",
        [
            "%2e%2e%2f%2e%2e%2fetc%2fpasswd",
            "%2e%2e%5c%2e%2e%5cwindows",
            "..%2Fetc%2Fpasswd",
            "..%5Cwindows%5Csystem32",
        ],
    )
    async def test_url_encoded_traversal_blocked(
        self, client: AsyncClient, auth_headers: dict, encoded_payload: str
    ):
        """Test that URL-encoded path traversal is blocked."""
        response = await client.get(
            "/api/v1/files/download", params={"path": encoded_payload}, headers=auth_headers
        )
        assert response.status_code in [400, 403, 404, 422]

    @pytest.mark.asyncio()
    @pytest.mark.parametrize(
        "double_encoded",
        [
            "%252e%252e%252f",
            "%252e%252e%255c",
        ],
    )
    async def test_double_url_encoded_traversal_blocked(
        self, client: AsyncClient, auth_headers: dict, double_encoded: str
    ):
        """Test that double URL-encoded traversal is blocked."""
        response = await client.get(
            "/api/v1/files/download",
            params={"path": double_encoded + "etc/passwd"},
            headers=auth_headers,
        )
        assert response.status_code in [400, 403, 404, 422]
