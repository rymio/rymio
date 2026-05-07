"""Tests for command execution actions in app.py."""

import asyncio
from pathlib import Path
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from litecode_agent.app import LitecodeApp
from litecode_agent.config import AppConfig


@pytest.fixture
def app(tmp_path):
    """Create a LitecodeApp instance for testing."""
    config = AppConfig()
    app = LitecodeApp(root_directory=tmp_path, config=config)
    return app


class TestExecuteCommand:
    """Tests for _execute_command helper method."""

    def test_execute_command_stores_last_command(self, app, tmp_path):
        """_execute_command stores the command in _last_command."""
        terminal_mock = MagicMock()
        terminal_mock.write = MagicMock()

        with patch.object(app, "query_one", return_value=terminal_mock):
            with patch(
                "litecode_agent.app.run_command",
                new_callable=lambda: lambda *a, **kw: AsyncMock(return_value=("output\n", 0)),
            ) as mock_run:
                mock_run_cmd = AsyncMock(return_value=("output\n", 0))
                with patch("litecode_agent.app.run_command", mock_run_cmd):
                    asyncio.run(app._execute_command("echo hello"))

        assert app._last_command == "echo hello"

    def test_execute_command_displays_output(self, app, tmp_path):
        """_execute_command displays command output and exit code."""
        terminal_mock = MagicMock()
        writes = []
        terminal_mock.write = lambda msg: writes.append(msg)

        with patch.object(app, "query_one", return_value=terminal_mock):
            with patch(
                "litecode_agent.app.run_command",
                AsyncMock(return_value=("test output\n", 0)),
            ):
                asyncio.run(app._execute_command("echo test"))

        assert "$ echo test" in writes
        assert "test output" in writes
        assert "Exit code: 0" in writes

    def test_execute_command_dangerous_warning(self, app, tmp_path):
        """_execute_command shows warning for dangerous commands."""
        terminal_mock = MagicMock()
        writes = []
        terminal_mock.write = lambda msg: writes.append(msg)

        with patch.object(app, "query_one", return_value=terminal_mock):
            with patch(
                "litecode_agent.app.run_command",
                AsyncMock(return_value=("", 0)),
            ):
                asyncio.run(app._execute_command("rm -rf /tmp/test"))

        assert any("Dangerous command" in w for w in writes)
        assert any("Proceeding with caution" in w for w in writes)

    def test_execute_command_safe_no_warning(self, app, tmp_path):
        """_execute_command does not show warning for safe commands."""
        terminal_mock = MagicMock()
        writes = []
        terminal_mock.write = lambda msg: writes.append(msg)

        with patch.object(app, "query_one", return_value=terminal_mock):
            with patch(
                "litecode_agent.app.run_command",
                AsyncMock(return_value=("", 0)),
            ):
                asyncio.run(app._execute_command("echo safe"))

        assert not any("Dangerous command" in w for w in writes)

    def test_execute_command_nonzero_exit(self, app, tmp_path):
        """_execute_command displays non-zero exit code."""
        terminal_mock = MagicMock()
        writes = []
        terminal_mock.write = lambda msg: writes.append(msg)

        with patch.object(app, "query_one", return_value=terminal_mock):
            with patch(
                "litecode_agent.app.run_command",
                AsyncMock(return_value=("error msg\n", 1)),
            ):
                asyncio.run(app._execute_command("false"))

        assert "Exit code: 1" in writes

    def test_execute_command_empty_output(self, app, tmp_path):
        """_execute_command handles empty output gracefully."""
        terminal_mock = MagicMock()
        writes = []
        terminal_mock.write = lambda msg: writes.append(msg)

        with patch.object(app, "query_one", return_value=terminal_mock):
            with patch(
                "litecode_agent.app.run_command",
                AsyncMock(return_value=("", 0)),
            ):
                asyncio.run(app._execute_command("true"))

        # Should have the command line and exit code, but no output line
        assert "$ true" in writes
        assert "Exit code: 0" in writes
        # Empty output should not produce an output line
        output_writes = [w for w in writes if w not in ("$ true", "Exit code: 0")]
        assert len(output_writes) == 0


class TestActionGitDiff:
    """Tests for action_git_diff."""

    def test_git_diff_calls_execute_command(self, app):
        """action_git_diff executes 'git diff'."""
        with patch.object(app, "_execute_command", new_callable=AsyncMock) as mock_exec:
            asyncio.run(app.action_git_diff())
            mock_exec.assert_called_once_with("git diff")


class TestActionRerunCommand:
    """Tests for action_rerun_command."""

    def test_rerun_with_no_previous_command(self, app):
        """action_rerun_command shows warning when no previous command."""
        terminal_mock = MagicMock()
        writes = []
        terminal_mock.write = lambda msg: writes.append(msg)

        with patch.object(app, "query_one", return_value=terminal_mock):
            asyncio.run(app.action_rerun_command())

        assert any("No previous command" in w for w in writes)

    def test_rerun_with_previous_command(self, app):
        """action_rerun_command re-runs the last command."""
        app._last_command = "echo hello"
        terminal_mock = MagicMock()

        with patch.object(app, "query_one", return_value=terminal_mock):
            with patch.object(app, "_execute_command", new_callable=AsyncMock) as mock_exec:
                asyncio.run(app.action_rerun_command())
                mock_exec.assert_called_once_with("echo hello")


class TestActionApplyPatch:
    """Tests for action_apply_patch."""

    def test_apply_patch_no_pending(self, app):
        """action_apply_patch shows warning when no pending patch."""
        terminal_mock = MagicMock()
        writes = []
        terminal_mock.write = lambda msg: writes.append(msg)

        with patch.object(app, "query_one", return_value=terminal_mock):
            asyncio.run(app.action_apply_patch())

        assert any("No patch to accept" in w for w in writes)

    def test_apply_patch_success(self, app, tmp_path):
        """action_apply_patch applies patch and shows success message."""
        # Create a file and set it as selected
        test_file = tmp_path / "test.py"
        test_file.write_text("original content")
        app._selected_file = test_file
        app._selected_file_content = "original content"

        # Store a proposal so has_pending_patch is True
        from litecode_agent.patches import PatchProposal
        app.patch_system.store_proposal(PatchProposal(
            target_file=test_file,
            diff_text="--- a/test.py\n+++ b/test.py\n@@ -1 +1 @@\n-original\n+new",
            original_content="original content",
            proposed_content="new content",
        ))

        terminal_mock = MagicMock()
        editor_mock = MagicMock()
        writes = []
        terminal_mock.write = lambda msg: writes.append(msg)

        def mock_query_one(selector, widget_type=None):
            if selector == "#terminal-log":
                return terminal_mock
            if selector == "#editor":
                return editor_mock
            return MagicMock()

        with patch.object(
            app.patch_system, "apply_patch", return_value=(True, "Patch applied. Backup at test.py.bak")
        ):
            with patch("litecode_agent.app.read_file_safe", return_value=("new content", None)):
                with patch.object(app, "query_one", side_effect=mock_query_one):
                    asyncio.run(app.action_apply_patch())

        assert any("✓" in w and "Patch applied" in w for w in writes)
        assert app._selected_file_content == "new content"
        editor_mock.load_text.assert_called_once_with("new content")

    def test_apply_patch_failure(self, app, tmp_path):
        """action_apply_patch shows failure message on error."""
        # Store a proposal so has_pending_patch is True
        from litecode_agent.patches import PatchProposal
        test_file = tmp_path / "test.py"
        test_file.write_text("original content")
        app.patch_system.store_proposal(PatchProposal(
            target_file=test_file,
            diff_text="--- a/test.py\n+++ b/test.py\n@@ -1 +1 @@\n-original\n+new",
            original_content="original content",
            proposed_content="new content",
        ))

        terminal_mock = MagicMock()
        writes = []
        terminal_mock.write = lambda msg: writes.append(msg)

        with patch.object(
            app.patch_system, "apply_patch", return_value=(False, "Patch failed: file not found. Original file restored.")
        ):
            with patch.object(app, "query_one", return_value=terminal_mock):
                asyncio.run(app.action_apply_patch())

        assert any("✗" in w and "Patch failed" in w for w in writes)

    def test_apply_patch_success_no_selected_file(self, app, tmp_path):
        """action_apply_patch succeeds without reloading editor when no file selected."""
        app._selected_file = None
        # Store a proposal so has_pending_patch is True
        from litecode_agent.patches import PatchProposal
        test_file = tmp_path / "test.py"
        test_file.write_text("original content")
        app.patch_system.store_proposal(PatchProposal(
            target_file=test_file,
            diff_text="--- a/test.py\n+++ b/test.py\n@@ -1 +1 @@\n-original\n+new",
            original_content="original content",
            proposed_content="new content",
        ))

        terminal_mock = MagicMock()
        writes = []
        terminal_mock.write = lambda msg: writes.append(msg)

        with patch.object(
            app.patch_system, "apply_patch", return_value=(True, "Patch applied. Backup at test.py.bak")
        ):
            with patch.object(app, "query_one", return_value=terminal_mock):
                asyncio.run(app.action_apply_patch())

        assert any("✓" in w and "Patch applied" in w for w in writes)
