"""Tests for search handling in app.py."""

import asyncio
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from litecode_agent.app import LitecodeApp
from litecode_agent.config import AppConfig
from litecode_agent.tasks import RouteResult
from litecode_agent.tools import SearchResult


@pytest.fixture
def app(tmp_path):
    """Create a LitecodeApp instance for testing."""
    config = AppConfig()
    app = LitecodeApp(root_directory=tmp_path, config=config)
    return app


class TestSearchHandler:
    """Tests for the search handler in _dispatch_handler."""

    def test_search_displays_results(self, app, tmp_path):
        """Search handler displays matching results in terminal pane."""
        route = RouteResult(handler_name="search", extracted_params={"term": "hello"})
        mock_results = [
            SearchResult(file_path=Path("src/main.py"), line_number=10, line_content="print('hello world')"),
            SearchResult(file_path=Path("src/utils.py"), line_number=5, line_content="# hello helper"),
        ]

        terminal_mock = MagicMock()
        writes = []
        terminal_mock.write = lambda msg: writes.append(msg)

        with patch.object(app, "query_one", return_value=terminal_mock):
            with patch("litecode_agent.app.search_files", return_value=mock_results):
                asyncio.run(app._dispatch_handler(route, "/search hello"))

        assert any("Found 2 match(es)" in w for w in writes)
        assert any("src/main.py:10" in w for w in writes)
        assert any("src/utils.py:5" in w for w in writes)

    def test_search_no_matches(self, app, tmp_path):
        """Search handler displays 'No matches found.' when results are empty."""
        route = RouteResult(handler_name="search", extracted_params={"term": "nonexistent"})

        terminal_mock = MagicMock()
        writes = []
        terminal_mock.write = lambda msg: writes.append(msg)

        with patch.object(app, "query_one", return_value=terminal_mock):
            with patch("litecode_agent.app.search_files", return_value=[]):
                asyncio.run(app._dispatch_handler(route, "/search nonexistent"))

        assert "No matches found." in writes

    def test_search_uses_correct_params(self, app, tmp_path):
        """Search handler passes root_directory, term, and ignored_directories to search_files."""
        route = RouteResult(handler_name="search", extracted_params={"term": "my_term"})

        terminal_mock = MagicMock()
        terminal_mock.write = MagicMock()

        with patch.object(app, "query_one", return_value=terminal_mock):
            with patch("litecode_agent.app.search_files", return_value=[]) as mock_search:
                asyncio.run(app._dispatch_handler(route, "/search my_term"))

        mock_search.assert_called_once_with(
            tmp_path, "my_term", app.config.ignored_directories
        )

    def test_search_displays_line_content(self, app, tmp_path):
        """Search handler displays file path, line number, and line content."""
        route = RouteResult(handler_name="search", extracted_params={"term": "TODO"})
        mock_results = [
            SearchResult(file_path=Path("app.py"), line_number=42, line_content="# TODO: fix this"),
        ]

        terminal_mock = MagicMock()
        writes = []
        terminal_mock.write = lambda msg: writes.append(msg)

        with patch.object(app, "query_one", return_value=terminal_mock):
            with patch("litecode_agent.app.search_files", return_value=mock_results):
                asyncio.run(app._dispatch_handler(route, "/search TODO"))

        assert any("app.py:42" in w and "# TODO: fix this" in w for w in writes)

    def test_search_empty_term_defaults(self, app, tmp_path):
        """Search handler handles missing term gracefully with empty string default."""
        route = RouteResult(handler_name="search", extracted_params={})

        terminal_mock = MagicMock()
        terminal_mock.write = MagicMock()

        with patch.object(app, "query_one", return_value=terminal_mock):
            with patch("litecode_agent.app.search_files", return_value=[]) as mock_search:
                asyncio.run(app._dispatch_handler(route, "/search"))

        mock_search.assert_called_once_with(
            tmp_path, "", app.config.ignored_directories
        )
