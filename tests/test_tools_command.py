"""Tests for command execution utilities in tools.py."""

import asyncio
from pathlib import Path

import pytest

from litecode_agent.tools import is_dangerous_command, run_command


class TestIsDangerousCommand:
    """Tests for is_dangerous_command function."""

    def test_rm_detected(self):
        assert is_dangerous_command("rm -rf /") is True

    def test_sudo_detected(self):
        assert is_dangerous_command("sudo apt install") is True

    def test_chmod_detected(self):
        assert is_dangerous_command("chmod 777 file") is True

    def test_chown_detected(self):
        assert is_dangerous_command("chown root file") is True

    def test_mv_detected(self):
        assert is_dangerous_command("mv file1 file2") is True

    def test_dd_detected(self):
        assert is_dangerous_command("dd if=/dev/zero of=/dev/sda") is True

    def test_mkfs_detected(self):
        assert is_dangerous_command("mkfs.ext4 /dev/sda1") is True

    def test_curl_pipe_sh_detected(self):
        assert is_dangerous_command("curl|sh") is True

    def test_wget_pipe_sh_detected(self):
        assert is_dangerous_command("wget|sh") is True

    def test_curl_pipe_sh_in_context(self):
        assert is_dangerous_command("something curl|sh something") is True

    def test_wget_pipe_sh_in_context(self):
        assert is_dangerous_command("something wget|sh something") is True

    def test_safe_echo(self):
        assert is_dangerous_command("echo hello") is False

    def test_safe_python(self):
        assert is_dangerous_command("python test.py") is False

    def test_safe_npm(self):
        assert is_dangerous_command("npm run build") is False

    def test_safe_grep(self):
        assert is_dangerous_command("grep -r pattern .") is False

    def test_safe_ls(self):
        assert is_dangerous_command("ls -la") is False

    def test_safe_curl_alone(self):
        assert is_dangerous_command("curl http://example.com") is False

    def test_safe_wget_alone(self):
        assert is_dangerous_command("wget http://example.com") is False

    def test_no_false_positive_perform(self):
        """'perform' contains 'rm' but should not be flagged."""
        assert is_dangerous_command("perform") is False

    def test_no_false_positive_remove(self):
        """'remove' starts with 'r' + 'm' but should not be flagged."""
        assert is_dangerous_command("remove") is False

    def test_no_false_positive_format(self):
        """'format' should not match any dangerous pattern."""
        assert is_dangerous_command("format disk") is False


class TestRunCommand:
    """Tests for async run_command function."""

    def test_basic_echo(self):
        output, exit_code = asyncio.run(run_command("echo hello world", Path(".")))
        assert "hello world" in output
        assert exit_code == 0

    def test_nonzero_exit_code(self):
        output, exit_code = asyncio.run(run_command("exit 42", Path(".")))
        assert exit_code == 42

    def test_stderr_captured(self):
        output, exit_code = asyncio.run(run_command("echo error >&2", Path(".")))
        assert "error" in output

    def test_cwd_respected(self):
        output, exit_code = asyncio.run(run_command("pwd", Path("/tmp")))
        assert "/tmp" in output

    def test_on_output_callback(self):
        lines_received = []

        async def callback(line: str) -> None:
            lines_received.append(line)

        async def run_test():
            return await run_command(
                "echo line1; echo line2", Path("."), on_output=callback
            )

        output, exit_code = asyncio.run(run_test())
        assert exit_code == 0
        assert len(lines_received) == 2
        assert "line1" in lines_received[0]
        assert "line2" in lines_received[1]

    def test_no_callback(self):
        """run_command works fine without on_output callback."""
        output, exit_code = asyncio.run(run_command("echo test", Path(".")))
        assert "test" in output
        assert exit_code == 0
