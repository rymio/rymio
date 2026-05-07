"""Unit tests for tools.py search functionality."""

from pathlib import Path

import pytest

from litecode_agent.tools import SearchResult, search_files


class TestSearchFiles:
    """Tests for search_files."""

    def test_finds_matching_term_in_file(self, tmp_path: Path) -> None:
        f = tmp_path / "hello.py"
        f.write_text("print('hello world')\n")
        results = search_files(tmp_path, "hello", [])
        assert len(results) == 1
        assert results[0].line_number == 1
        assert "hello world" in results[0].line_content

    def test_returns_relative_paths(self, tmp_path: Path) -> None:
        sub = tmp_path / "src"
        sub.mkdir()
        f = sub / "main.py"
        f.write_text("import os\n")
        results = search_files(tmp_path, "import", [])
        assert results[0].file_path == Path("src/main.py")

    def test_case_insensitive_matching(self, tmp_path: Path) -> None:
        f = tmp_path / "test.txt"
        f.write_text("Hello World\n")
        results = search_files(tmp_path, "hello world", [])
        assert len(results) == 1

    def test_skips_ignored_directories(self, tmp_path: Path) -> None:
        ignored = tmp_path / "node_modules"
        ignored.mkdir()
        (ignored / "pkg.js").write_text("function hello() {}\n")
        visible = tmp_path / "src"
        visible.mkdir()
        (visible / "app.js").write_text("function hello() {}\n")
        results = search_files(tmp_path, "hello", ["node_modules"])
        assert len(results) == 1
        assert "node_modules" not in str(results[0].file_path)

    def test_skips_binary_files(self, tmp_path: Path) -> None:
        f = tmp_path / "data.bin"
        f.write_bytes(b"hello\x00world")
        results = search_files(tmp_path, "hello", [])
        assert len(results) == 0

    def test_truncates_line_content_to_120_chars(self, tmp_path: Path) -> None:
        long_line = "x" * 50 + "needle" + "y" * 200
        f = tmp_path / "long.txt"
        f.write_text(long_line + "\n")
        results = search_files(tmp_path, "needle", [])
        assert len(results) == 1
        assert len(results[0].line_content) == 120

    def test_respects_max_results_limit(self, tmp_path: Path) -> None:
        f = tmp_path / "many.txt"
        f.write_text("match\n" * 200)
        results = search_files(tmp_path, "match", [], max_results=5)
        assert len(results) == 5

    def test_no_matches_returns_empty_list(self, tmp_path: Path) -> None:
        f = tmp_path / "file.txt"
        f.write_text("nothing here\n")
        results = search_files(tmp_path, "zebra", [])
        assert results == []

    def test_multiple_matches_in_same_file(self, tmp_path: Path) -> None:
        f = tmp_path / "multi.txt"
        f.write_text("line one foo\nline two\nline three foo\n")
        results = search_files(tmp_path, "foo", [])
        assert len(results) == 2
        assert results[0].line_number == 1
        assert results[1].line_number == 3

    def test_skips_non_file_entries(self, tmp_path: Path) -> None:
        d = tmp_path / "subdir"
        d.mkdir()
        results = search_files(tmp_path, "subdir", [])
        assert len(results) == 0

    def test_strips_whitespace_from_line_content(self, tmp_path: Path) -> None:
        f = tmp_path / "spaces.txt"
        f.write_text("   hello   \n")
        results = search_files(tmp_path, "hello", [])
        assert results[0].line_content == "hello"
