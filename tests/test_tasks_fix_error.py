"""Unit tests for the fix_error handler."""

import pytest
from pathlib import Path
from unittest.mock import AsyncMock, MagicMock

from litecode_agent.tasks import fix_error
from litecode_agent.llm import LLMResponse
from litecode_agent.patches import PatchSystem, PatchProposal


@pytest.fixture
def mock_llm():
    llm = AsyncMock()
    llm.chat.return_value = LLMResponse(
        content="```diff\n--- a/main.py\n+++ b/main.py\n@@ -42,3 +42,3 @@\n-    x = 1/0\n+    x = 1\n```",
        finish_reason="stop",
    )
    return llm


@pytest.fixture
def mock_patch_system():
    ps = MagicMock(spec=PatchSystem)
    ps.parse_llm_diff.return_value = PatchProposal(
        target_file=Path("/project/main.py"),
        diff_text="--- a/main.py\n+++ b/main.py\n@@ -42,3 +42,3 @@\n-    x = 1/0\n+    x = 1\n",
        original_content="line\n" * 100,
        proposed_content="fixed\n" * 100,
    )
    ps.get_diff_display.return_value = "--- a/main.py\n+++ b/main.py\n@@ -42,3 +42,3 @@\n-    x = 1/0\n+    x = 1\n"
    return ps


@pytest.fixture
def mock_ctx():
    ctx = AsyncMock()
    ctx.selected_file_path = Path("/project/main.py")
    ctx.selected_file_content = "\n".join(f"line {i}" for i in range(1, 101))
    ctx.display_chat = AsyncMock()
    ctx.display_terminal = AsyncMock()
    ctx.set_status = AsyncMock()
    return ctx


@pytest.mark.asyncio
async def test_no_file_selected(mock_llm, mock_patch_system):
    ctx = AsyncMock()
    ctx.selected_file_path = None
    ctx.selected_file_content = None
    ctx.display_chat = AsyncMock()

    await fix_error(ctx, mock_llm, mock_patch_system, {"line_number": 42})

    ctx.display_chat.assert_called_once_with("Please select a file first.")
    mock_llm.chat.assert_not_called()


@pytest.mark.asyncio
async def test_no_line_number(mock_ctx, mock_llm, mock_patch_system):
    await fix_error(mock_ctx, mock_llm, mock_patch_system, {})

    mock_ctx.display_chat.assert_called_once_with(
        "Please specify a line number (e.g., 'fix line 42')."
    )
    mock_llm.chat.assert_not_called()


@pytest.mark.asyncio
async def test_line_number_none_in_params(mock_ctx, mock_llm, mock_patch_system):
    await fix_error(mock_ctx, mock_llm, mock_patch_system, {"line_number": None})

    mock_ctx.display_chat.assert_called_once_with(
        "Please specify a line number (e.g., 'fix line 42')."
    )
    mock_llm.chat.assert_not_called()


@pytest.mark.asyncio
async def test_extracts_snippet_around_target_line(mock_ctx, mock_llm, mock_patch_system):
    params = {"line_number": 50, "error_text": "ZeroDivisionError"}

    await fix_error(mock_ctx, mock_llm, mock_patch_system, params)

    # Verify LLM was called
    mock_llm.chat.assert_called_once()
    sent_messages = mock_llm.chat.call_args[0][0]
    user_msg = sent_messages[1].content

    # The snippet should contain numbered lines around line 50
    # ±40 means lines 10-90 (line_number - 41 to line_number + 40)
    assert "  10:" in user_msg  # start line
    assert "  50:" in user_msg  # target line
    assert "  90:" in user_msg  # end line


@pytest.mark.asyncio
async def test_snippet_clamps_at_file_start(mock_ctx, mock_llm, mock_patch_system):
    params = {"line_number": 5, "error_text": "SyntaxError"}

    await fix_error(mock_ctx, mock_llm, mock_patch_system, params)

    mock_llm.chat.assert_called_once()
    sent_messages = mock_llm.chat.call_args[0][0]
    user_msg = sent_messages[1].content

    # Should start from line 1 (clamped at 0)
    assert "   1:" in user_msg


@pytest.mark.asyncio
async def test_snippet_clamps_at_file_end(mock_ctx, mock_llm, mock_patch_system):
    params = {"line_number": 95, "error_text": "IndexError"}

    await fix_error(mock_ctx, mock_llm, mock_patch_system, params)

    mock_llm.chat.assert_called_once()
    sent_messages = mock_llm.chat.call_args[0][0]
    user_msg = sent_messages[1].content

    # Should include line 100 (last line) but not beyond
    assert " 100:" in user_msg


@pytest.mark.asyncio
async def test_valid_diff_stored_and_displayed(mock_ctx, mock_llm, mock_patch_system):
    params = {"line_number": 42, "error_text": "ZeroDivisionError"}

    await fix_error(mock_ctx, mock_llm, mock_patch_system, params)

    # Should parse the diff
    mock_patch_system.parse_llm_diff.assert_called_once()

    # Should store the proposal
    mock_patch_system.store_proposal.assert_called_once()

    # Should display diff in terminal pane
    mock_ctx.display_terminal.assert_called_once()

    # Should inform user about the fix
    chat_msg = mock_ctx.display_chat.call_args[0][0]
    assert "F5" in chat_msg
    assert "Accept" in chat_msg


@pytest.mark.asyncio
async def test_no_valid_diff_displays_response(mock_ctx, mock_llm, mock_patch_system):
    mock_patch_system.parse_llm_diff.return_value = None
    mock_llm.chat.return_value = LLMResponse(
        content="I couldn't generate a diff, but here's a suggestion...",
        finish_reason="stop",
    )
    params = {"line_number": 42, "error_text": "TypeError"}

    await fix_error(mock_ctx, mock_llm, mock_patch_system, params)

    # Should NOT store or display diff
    mock_patch_system.store_proposal.assert_not_called()
    mock_ctx.display_terminal.assert_not_called()

    # Should display the raw response in chat
    mock_ctx.display_chat.assert_called_once_with(
        "I couldn't generate a diff, but here's a suggestion..."
    )


@pytest.mark.asyncio
async def test_sends_correct_prompt_structure(mock_ctx, mock_llm, mock_patch_system):
    params = {"line_number": 42, "error_text": "NameError: name 'foo' is not defined"}

    await fix_error(mock_ctx, mock_llm, mock_patch_system, params)

    sent_messages = mock_llm.chat.call_args[0][0]
    assert len(sent_messages) == 2
    assert sent_messages[0].role == "system"
    assert sent_messages[1].role == "user"
    # Should contain file path, error text, and line number
    user_content = sent_messages[1].content
    assert "main.py" in user_content
    assert "42" in user_content


@pytest.mark.asyncio
async def test_passes_on_status_callback(mock_ctx, mock_llm, mock_patch_system):
    params = {"line_number": 42, "error_text": "error"}

    await fix_error(mock_ctx, mock_llm, mock_patch_system, params)

    call_kwargs = mock_llm.chat.call_args[1]
    assert call_kwargs["on_status"] == mock_ctx.set_status


@pytest.mark.asyncio
async def test_sets_status_before_llm_call(mock_ctx, mock_llm, mock_patch_system):
    params = {"line_number": 42, "error_text": "error"}

    await fix_error(mock_ctx, mock_llm, mock_patch_system, params)

    mock_ctx.set_status.assert_any_call("Requesting fix...")


@pytest.mark.asyncio
async def test_does_not_auto_apply_patch(mock_ctx, mock_llm, mock_patch_system):
    """Verify the handler does NOT auto-apply the patch (Req 8.7)."""
    params = {"line_number": 42, "error_text": "error"}

    await fix_error(mock_ctx, mock_llm, mock_patch_system, params)

    # apply_patch should never be called
    mock_patch_system.apply_patch.assert_not_called()
