"""Unit tests for tools.py file utility functions."""

import os
from pathlib import Path

import pytest

from litecode_agent.tools import (
    is_binary_file,
    is_secret_file,
    is_within_root,
    read_file_safe,
)


class TestIsBinaryFile:
    """Tests for is_binary_file."""

    def test_text_file_is_not_binary(self, tmp_path: Path) -> None:
        f = tmp_path / "hello.txt"
        f.write_text("Hello, world!\n")
        assert is_binary_file(f) is False

    def test_file_with_null_bytes_is_binary(self, tmp_path: Path) -> None:
        f = tmp_path / "data.bin"
        f.write_bytes(b"some data\x00more data")
        assert is_binary_file(f) is True

    def test_empty_file_is_not_binary(self, tmp_path: Path) -> None:
        f = tmp_path / "empty.txt"
        f.write_bytes(b"")
        assert is_binary_file(f) is False

    def test_nonexistent_file_returns_false(self, tmp_path: Path) -> None:
        f = tmp_path / "missing.txt"
        assert is_binary_file(f) is False

    def test_null_byte_beyond_8kb_not_detected(self, tmp_path: Path) -> None:
        f = tmp_path / "large.txt"
        # Write 8KB of text followed by a null byte
        content = b"A" * 8192 + b"\x00"
        f.write_bytes(content)
        assert is_binary_file(f) is False


class TestIsSecretFile:
    """Tests for is_secret_file."""

    PATTERNS = [".env", "id_rsa", "*.pem", "*.key", "settings_local.py"]

    def test_env_file_matches(self) -> None:
        assert is_secret_file(Path("/project/.env"), self.PATTERNS) is True

    def test_pem_file_matches(self) -> None:
        assert is_secret_file(Path("/certs/server.pem"), self.PATTERNS) is True

    def test_key_file_matches(self) -> None:
        assert is_secret_file(Path("/keys/private.key"), self.PATTERNS) is True

    def test_id_rsa_matches(self) -> None:
        assert is_secret_file(Path("/home/user/.ssh/id_rsa"), self.PATTERNS) is True

    def test_settings_local_matches(self) -> None:
        assert is_secret_file(Path("/project/settings_local.py"), self.PATTERNS) is True

    def test_regular_python_file_does_not_match(self) -> None:
        assert is_secret_file(Path("/project/main.py"), self.PATTERNS) is False

    def test_regular_text_file_does_not_match(self) -> None:
        assert is_secret_file(Path("/project/readme.md"), self.PATTERNS) is False

    def test_empty_patterns_matches_nothing(self) -> None:
        assert is_secret_file(Path("/project/.env"), []) is False


class TestIsWithinRoot:
    """Tests for is_within_root."""

    def test_file_inside_root(self, tmp_path: Path) -> None:
        child = tmp_path / "src" / "main.py"
        child.parent.mkdir(parents=True, exist_ok=True)
        child.touch()
        assert is_within_root(child, tmp_path) is True

    def test_file_outside_root(self, tmp_path: Path) -> None:
        root = tmp_path / "project"
        root.mkdir()
        outside = tmp_path / "other" / "file.txt"
        outside.parent.mkdir(parents=True, exist_ok=True)
        outside.touch()
        assert is_within_root(outside, root) is False

    def test_path_traversal_attempt(self, tmp_path: Path) -> None:
        root = tmp_path / "project"
        root.mkdir()
        traversal = root / ".." / "other" / "file.txt"
        assert is_within_root(traversal, root) is False

    def test_root_itself_is_within_root(self, tmp_path: Path) -> None:
        assert is_within_root(tmp_path, tmp_path) is True

    def test_symlink_outside_root_detected(self, tmp_path: Path) -> None:
        root = tmp_path / "project"
        root.mkdir()
        outside = tmp_path / "secret.txt"
        outside.write_text("secret")
        link = root / "link.txt"
        link.symlink_to(outside)
        assert is_within_root(link, root) is False


class TestReadFileSafe:
    """Tests for read_file_safe."""

    def test_read_normal_text_file(self, tmp_path: Path) -> None:
        f = tmp_path / "hello.py"
        f.write_text("print('hello')\n")
        content, error = read_file_safe(f)
        assert content == "print('hello')\n"
        assert error is None

    def test_oversized_file_returns_error(self, tmp_path: Path) -> None:
        f = tmp_path / "big.txt"
        # Write a file larger than 1 KB (using max_kb=1 for test)
        f.write_text("x" * 2048)
        content, error = read_file_safe(f, max_kb=1)
        assert content is None
        assert "size limit" in error

    def test_binary_file_returns_error(self, tmp_path: Path) -> None:
        f = tmp_path / "data.bin"
        f.write_bytes(b"binary\x00content")
        content, error = read_file_safe(f)
        assert content is None
        assert "Binary file" in error

    def test_nonexistent_file_returns_error(self, tmp_path: Path) -> None:
        f = tmp_path / "missing.txt"
        content, error = read_file_safe(f)
        assert content is None
        assert "Cannot access" in error

    def test_empty_file_reads_successfully(self, tmp_path: Path) -> None:
        f = tmp_path / "empty.txt"
        f.write_text("")
        content, error = read_file_safe(f)
        assert content == ""
        assert error is None

    def test_size_check_uses_max_kb_parameter(self, tmp_path: Path) -> None:
        f = tmp_path / "medium.txt"
        f.write_text("a" * 500)
        # With max_kb=1 (1024 bytes), 500 bytes should be fine
        content, error = read_file_safe(f, max_kb=1)
        assert content is not None
        assert error is None
