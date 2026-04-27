"""Unit tests for API validators.

Tests comprehensive file validation including:
- Path traversal protection
- NULL byte injection protection
- Control character detection
- Absolute path detection
- Path separator detection
- Windows reserved name detection
"""

from fastapi import HTTPException
import pytest

from src.api.validators import validate_filename


class TestValidateFilename:
    """Test suite for validate_filename function."""

    @pytest.mark.parametrize(
        "valid_name",
        [
            "document.pdf",
            "image_2024.jpg",
            "my-file.txt",
            "file (1).doc",
            "report.xlsx",
            "data-2024-01-15.csv",
            "presentation_v2.pptx",
        ],
    )
    def test_valid_filenames(self, valid_name: str):
        """Test that valid filenames are accepted."""
        assert validate_filename(valid_name) == valid_name

    @pytest.mark.parametrize(
        "invalid_name",
        [
            "",
            "   ",
            "\t",
            "\n",
            "\t\n",
            " leading.txt",
            "trailing.txt ",
            " both.txt ",
        ],
    )
    def test_empty_or_whitespace_rejected(self, invalid_name: str):
        """Test that empty or whitespace filenames are rejected."""
        with pytest.raises(HTTPException) as exc_info:
            validate_filename(invalid_name)
        assert exc_info.value.status_code == 400

    def test_filename_too_long(self):
        """Test that filenames longer than 255 characters are rejected."""
        long_name = "a" * 256
        with pytest.raises(HTTPException) as exc_info:
            validate_filename(long_name)
        assert exc_info.value.status_code == 400
        assert "too long" in str(exc_info.value.detail).lower()

    @pytest.mark.parametrize(
        "dangerous_name",
        [
            "../etc/passwd",
            "..\\windows\\system32",
            "../../file.txt",
            "..\\..\\file.txt",
            "dir/../file.txt",
            "dir\\..\\file.txt",
            ".../.../file.txt",
        ],
    )
    def test_path_traversal_blocked(self, dangerous_name: str):
        """Test that filenames with path traversal patterns are rejected."""
        with pytest.raises(HTTPException) as exc_info:
            validate_filename(dangerous_name)
        assert exc_info.value.status_code == 400
        assert "path traversal" in str(exc_info.value.detail).lower()

    @pytest.mark.parametrize(
        "filename",
        [
            "path/to/file.txt",
            "path\\to\\file.txt",
            "dir/file.txt",
            "dir\\file.txt",
            "/file.txt",
            "\\file.txt",
        ],
    )
    def test_path_separators_rejected(self, filename: str):
        """Test that path separators are rejected."""
        with pytest.raises(HTTPException) as exc_info:
            validate_filename(filename)
        assert exc_info.value.status_code == 400
        assert "path separator" in str(exc_info.value.detail).lower()

    def test_null_byte_rejected(self):
        """Test that null bytes are rejected."""
        with pytest.raises(HTTPException) as exc_info:
            validate_filename("file\x00.txt")
        assert exc_info.value.status_code == 400
        assert "null byte" in str(exc_info.value.detail).lower()

    @pytest.mark.parametrize(
        "control_char",
        [
            "\x00",
            "\x01",
            "\x02",
            "\x03",
            "\x0b",
            "\x0c",
            "\x0e",
            "\x0f",
            "\x1f",
        ],
    )
    def test_control_characters_rejected(self, control_char: str):
        """Test that control characters are rejected."""
        filename = f"file{control_char}.txt"
        with pytest.raises(HTTPException) as exc_info:
            validate_filename(filename)
        assert exc_info.value.status_code == 400
        assert "control character" in str(exc_info.value.detail).lower()

    @pytest.mark.parametrize(
        "reserved_name",
        [
            "CON.txt",
            "PRN.doc",
            "AUX.pdf",
            "NUL.txt",
            "COM1.txt",
            "COM2.doc",
            "COM3.pdf",
            "COM4.txt",
            "COM5.doc",
            "COM6.pdf",
            "COM7.txt",
            "COM8.doc",
            "COM9.pdf",
            "LPT1.txt",
            "LPT2.doc",
            "LPT3.pdf",
            "LPT4.txt",
            "LPT5.doc",
            "LPT6.pdf",
            "LPT7.txt",
            "LPT8.doc",
            "LPT9.pdf",
            "con.txt",
            "prn.doc",
            "aux.pdf",
        ],
    )
    def test_windows_reserved_names_rejected(self, reserved_name: str):
        """Test that Windows reserved names are rejected."""
        with pytest.raises(HTTPException) as exc_info:
            validate_filename(reserved_name)
        assert exc_info.value.status_code == 400
        assert "reserved Windows name" in str(exc_info.value.detail).lower()

    def test_tilde_home_directory_blocked(self):
        """Test that tilde (~) is blocked to prevent home directory access."""
        with pytest.raises(HTTPException) as exc_info:
            validate_filename("~/secret.txt")
        assert exc_info.value.status_code == 400
        assert "path traversal" in str(exc_info.value.detail).lower()

    @pytest.mark.parametrize(
        "absolute_path",
        [
            "/etc/passwd",
            "/home/user/file.txt",
            "C:\\Windows\\System32\\config\\sam",
            "C:/Windows/System32/config/sam",
        ],
    )
    def test_absolute_paths_rejected(self, absolute_path: str):
        """Test that absolute paths are rejected."""
        with pytest.raises(HTTPException) as exc_info:
            validate_filename(absolute_path)
        assert exc_info.value.status_code == 400

    def test_unicode_characters_allowed(self):
        """Test that unicode characters in filenames are allowed."""
        # These should be allowed
        valid_unicode_names = [
            "документ.pdf",
            "文档.txt",
            "ファイル.doc",
        ]
        for name in valid_unicode_names:
            assert validate_filename(name) == name

    def test_special_characters_allowed(self):
        """Test that some special characters are allowed in filenames."""
        valid_special_names = [
            "file-name.txt",
            "file_name.txt",
            "file (1).txt",
            "file[1].txt",
            "file{1}.txt",
            "file@name.txt",
            "file#1.txt",
            "file$.txt",
            "file%.txt",
            "file&name.txt",
        ]
        for name in valid_special_names:
            assert validate_filename(name) == name


class TestValidateFilenameEdgeCases:
    """Test edge cases for validate_filename."""

    def test_exactly_255_characters_allowed(self):
        """Test that filename with exactly 255 characters is allowed."""
        name = "a" * 251 + ".txt"  # 251 + 4 = 255
        assert len(name) == 255
        assert validate_filename(name) == name

    def test_256_characters_rejected(self):
        """Test that filename with 256 characters is rejected."""
        name = "a" * 252 + ".txt"  # 252 + 4 = 256
        assert len(name) == 256
        with pytest.raises(HTTPException) as exc_info:
            validate_filename(name)
        assert exc_info.value.status_code == 400

    def test_filename_with_multiple_dots(self):
        """Test that filenames with multiple dots are allowed."""
        name = "file.tar.gz"
        assert validate_filename(name) == name

    def test_filename_without_extension(self):
        """Test that filenames without extension are allowed."""
        name = "README"
        assert validate_filename(name) == name

    def test_filename_starting_with_dot(self):
        """Test that filenames starting with dot are allowed."""
        name = ".gitignore"
        assert validate_filename(name) == name

    def test_double_dot_at_start_blocked(self):
        """Test that filenames starting with .. are blocked."""
        with pytest.raises(HTTPException) as exc_info:
            validate_filename("..file.txt")
        assert exc_info.value.status_code == 400

    def test_reserved_name_case_insensitive(self):
        """Test that reserved names are checked case-insensitively."""
        for name in ["CON", "Con", "con", "cOn"]:
            with pytest.raises(HTTPException) as exc_info:
                validate_filename(f"{name}.txt")
            assert exc_info.value.status_code == 400


class TestValidateFilenameIntegration:
    """Integration tests for validate_filename with real-world scenarios."""

    def test_safe_filename_workflow(self):
        """Test a typical safe filename workflow."""
        filenames = [
            "user_upload_2024.pdf",
            "report-Q4-2024.xlsx",
            "meeting_notes_v2.docx",
            "image_001.jpg",
        ]
        for filename in filenames:
            validated = validate_filename(filename)
            assert validated == filename

    def test_dangerous_filename_workflow(self):
        """Test that all dangerous patterns are caught."""
        dangerous_filenames = [
            "../../../etc/passwd",
            "file\x00.txt",
            "\x01malicious.txt",
            "~/private.txt",
            "/etc/shadow",
            "C:\\Windows\\system32\\config\\sam",
            "CON.txt",
            "path/to/file.txt",
        ]
        for filename in dangerous_filenames:
            with pytest.raises(HTTPException):
                validate_filename(filename)

    def test_canonicalization_attacks(self):
        """Test protection against canonicalization attacks."""
        # Various encodings and representations that should all be blocked
        attacks = [
            "..\\..\\file.txt",
            "....//....//file.txt",
            "~/../file.txt",
        ]
        for attack in attacks:
            with pytest.raises(HTTPException):
                validate_filename(attack)
