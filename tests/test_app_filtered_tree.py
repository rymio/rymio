"""Tests for FilteredDirectoryTree widget."""
from pathlib import Path

from litecode_agent.app import FilteredDirectoryTree


class TestFilteredDirectoryTree:
    """Tests for the FilteredDirectoryTree filter_paths method."""

    def test_filters_ignored_directories(self, tmp_path):
        """Paths matching ignored_directories are excluded."""
        tree = FilteredDirectoryTree(tmp_path, ignored_directories=[".git", "node_modules", "__pycache__"])
        paths = [
            tmp_path / "src",
            tmp_path / ".git",
            tmp_path / "node_modules",
            tmp_path / "__pycache__",
            tmp_path / "main.py",
        ]
        result = list(tree.filter_paths(paths))
        assert result == [tmp_path / "src", tmp_path / "main.py"]

    def test_no_ignored_directories(self, tmp_path):
        """When no ignored_directories provided, all paths pass through."""
        tree = FilteredDirectoryTree(tmp_path, ignored_directories=None)
        paths = [tmp_path / "src", tmp_path / ".git", tmp_path / "main.py"]
        result = list(tree.filter_paths(paths))
        assert result == paths

    def test_empty_ignored_directories(self, tmp_path):
        """Empty ignored_directories list filters nothing."""
        tree = FilteredDirectoryTree(tmp_path, ignored_directories=[])
        paths = [tmp_path / "src", tmp_path / ".git"]
        result = list(tree.filter_paths(paths))
        assert result == paths

    def test_filters_only_matching_names(self, tmp_path):
        """Only exact name matches are filtered, not partial matches."""
        tree = FilteredDirectoryTree(tmp_path, ignored_directories=["build"])
        paths = [
            tmp_path / "build",
            tmp_path / "build_scripts",
            tmp_path / "rebuild",
        ]
        result = list(tree.filter_paths(paths))
        assert result == [tmp_path / "build_scripts", tmp_path / "rebuild"]

    def test_default_ignored_directories_from_config(self, tmp_path):
        """Verify filtering works with the default config ignored directories."""
        defaults = [".git", ".venv", "venv", "env", "node_modules", "__pycache__", "dist", "build"]
        tree = FilteredDirectoryTree(tmp_path, ignored_directories=defaults)
        paths = [
            tmp_path / ".git",
            tmp_path / "src",
            tmp_path / "node_modules",
            tmp_path / "README.md",
        ]
        result = list(tree.filter_paths(paths))
        assert result == [tmp_path / "src", tmp_path / "README.md"]
