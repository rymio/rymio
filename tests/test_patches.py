"""Tests for PatchSystem.apply_patch method."""

import pytest
from pathlib import Path

from litecode_agent.patches import PatchSystem, PatchProposal


class TestApplyPatchNoProposal:
    """Test apply_patch when no proposal is stored."""

    def test_returns_failure_when_no_patch_stored(self):
        ps = PatchSystem()
        success, message = ps.apply_patch()
        assert success is False
        assert message == "No patch to apply."


class TestApplyPatchWithBackup:
    """Test that apply_patch creates a .bak backup before writing."""

    def test_creates_backup_file(self, tmp_path):
        target = tmp_path / "example.py"
        target.write_text("original content\n")

        proposal = PatchProposal(
            target_file=target,
            diff_text="--- a/example.py\n+++ b/example.py\n@@ -1 +1 @@\n-original content\n+new content\n",
            original_content="original content\n",
            proposed_content="new content\n",
        )
        ps = PatchSystem()
        ps.store_proposal(proposal)

        ps.apply_patch()

        backup = tmp_path / "example.py.bak"
        assert backup.exists()
        assert backup.read_text() == "original content\n"


class TestApplyPatchFallback:
    """Test full-file replacement fallback when diff application fails."""

    def test_falls_back_to_full_replacement(self, tmp_path):
        target = tmp_path / "example.py"
        target.write_text("original content\n")

        # Use a diff with malformed hunk header (no valid @@ marker) so _apply_unified_diff returns None
        proposal = PatchProposal(
            target_file=target,
            diff_text="--- a/example.py\n+++ b/example.py\n@@ INVALID HUNK @@\n-nonexistent line\n+replaced\n",
            original_content="original content\n",
            proposed_content="replaced content via fallback\n",
        )
        ps = PatchSystem()
        ps.store_proposal(proposal)

        success, message = ps.apply_patch()

        assert success is True
        assert "Full replacement applied" in message
        assert target.read_text() == "replaced content via fallback\n"


class TestApplyPatchRestoreOnFailure:
    """Test that original file is restored from backup on failure."""

    def test_restores_from_backup_on_write_failure(self, tmp_path, monkeypatch):
        target = tmp_path / "example.py"
        target.write_text("original content\n")

        # Use a diff that won't apply (malformed hunk) so it falls through to fallback
        proposal = PatchProposal(
            target_file=target,
            diff_text="--- a/example.py\n+++ b/example.py\n@@ INVALID @@\n-x\n+y\n",
            original_content="original content\n",
            proposed_content="new content\n",
        )
        ps = PatchSystem()
        ps.store_proposal(proposal)

        # Monkeypatch Path.write_text to fail on the fallback write
        original_write_text = Path.write_text
        call_count = [0]

        def failing_write_text(self_path, *args, **kwargs):
            call_count[0] += 1
            # Let the first write_text call succeed (if any from diff apply)
            # but fail on the fallback write
            raise OSError("Simulated disk full")

        monkeypatch.setattr(Path, "write_text", failing_write_text)

        success, message = ps.apply_patch()

        assert success is False
        assert "Patch failed" in message
        assert "Original file restored" in message
        # Verify original content is preserved (backup was restored)
        monkeypatch.undo()
        assert target.read_text() == "original content\n"


class TestApplyPatchClearsProposal:
    """Test that stored proposal is cleared after successful application."""

    def test_clears_proposal_after_successful_patch(self, tmp_path):
        target = tmp_path / "example.py"
        target.write_text("hello\n")

        proposal = PatchProposal(
            target_file=target,
            diff_text="--- a/example.py\n+++ b/example.py\n@@ -1,1 +1,1 @@\n-hello\n+world\n",
            original_content="hello\n",
            proposed_content="world\n",
        )
        ps = PatchSystem()
        ps.store_proposal(proposal)
        assert ps.has_pending_patch is True

        ps.apply_patch()

        assert ps.has_pending_patch is False

    def test_clears_proposal_after_fallback_replacement(self, tmp_path):
        target = tmp_path / "example.py"
        target.write_text("original\n")

        proposal = PatchProposal(
            target_file=target,
            diff_text="--- a/example.py\n+++ b/example.py\n@@ -99,1 +99,1 @@\n-nonexistent\n+replaced\n",
            original_content="original\n",
            proposed_content="fallback content\n",
        )
        ps = PatchSystem()
        ps.store_proposal(proposal)

        ps.apply_patch()

        assert ps.has_pending_patch is False


class TestApplyPatchSuccess:
    """Test successful line-by-line diff application."""

    def test_applies_simple_line_replacement(self, tmp_path):
        target = tmp_path / "example.py"
        target.write_text("line1\nline2\nline3\n")

        diff_text = (
            "--- a/example.py\n"
            "+++ b/example.py\n"
            "@@ -1,3 +1,3 @@\n"
            " line1\n"
            "-line2\n"
            "+line2_modified\n"
            " line3\n"
        )
        proposal = PatchProposal(
            target_file=target,
            diff_text=diff_text,
            original_content="line1\nline2\nline3\n",
            proposed_content="line1\nline2_modified\nline3\n",
        )
        ps = PatchSystem()
        ps.store_proposal(proposal)

        success, message = ps.apply_patch()

        assert success is True
        assert "Patch applied" in message
        assert "Backup at" in message
