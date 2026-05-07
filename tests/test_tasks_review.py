"""Unit tests for the review_selected_file handler."""

import pytest
from pathlib import Path
from unittest.mock import AsyncMock, MagicMock

from litecode_agent.tasks import review_selected_file
from litecode_agent.llm import LLMResponse


@pytest.fixture
def mock_llm():
    llm = AsyncMock()
    llm.chat.return_value = LLMResponse(
        content="Looks good, no issues found.", finish_reason="stop"
    )
    return llm


@pytest.fixture
def mock_ctx():
    ctx = AsyncMock()
    ctx.selected_file_path = Path("/project/main.py")
    ctx.selected_file_content = "def hello():\n    print('hi')\n"
    ctx.display_chat = AsyncMock()
    ctx.set_status = AsyncMock()
    return ctx


@pytest.mark.asyncio
async def test_no_file_selected_path_none(mock_llm):
    ctx = AsyncMock()
    ctx.selected_file_path = None
    ctx.selected_file_content = None
    ctx.display_chat = AsyncMock()

    await review_selected_file(ctx, mock_llm)

    ctx.display_chat.assert_called_once_with("Please select a file first.")
    mock_llm.chat.assert_not_called()


@pytest.mark.asyncio
async def test_no_file_selected_content_none(mock_llm):
    ctx = AsyncMock()
    ctx.selected_file_path = Path("/project/main.py")
    ctx.selected_file_content = None
    ctx.display_chat = AsyncMock()

    await review_selected_file(ctx, mock_llm)

    ctx.display_chat.assert_called_once_with("Please select a file first.")
    mock_llm.chat.assert_not_called()


@pytest.mark.asyncio
async def test_file_within_300_lines(mock_ctx, mock_llm):
    mock_ctx.selected_file_content = "\n".join(
        [f"line {i}" for i in range(100)]
    )

    await review_selected_file(mock_ctx, mock_llm)

    # Should call LLM and display response
    mock_llm.chat.assert_called_once()
    mock_ctx.set_status.assert_any_call("Requesting code review...")
    mock_ctx.display_chat.assert_called_once_with("Looks good, no issues found.")


@pytest.mark.asyncio
async def test_file_exceeds_300_lines(mock_ctx, mock_llm):
    mock_ctx.selected_file_content = "\n".join(
        [f"line {i}" for i in range(500)]
    )

    await review_selected_file(mock_ctx, mock_llm)

    # Should inform user about truncation
    calls = mock_ctx.display_chat.call_args_list
    assert len(calls) == 2
    truncation_msg = calls[0][0][0]
    assert "500 lines" in truncation_msg
    assert "300 lines" in truncation_msg
    assert "narrow the scope" in truncation_msg

    # Should still call LLM with truncated content
    mock_llm.chat.assert_called_once()
    sent_messages = mock_llm.chat.call_args[0][0]
    user_msg_content = sent_messages[1].content
    # Only first 300 lines should be in the prompt
    assert "line 299" in user_msg_content
    assert "line 300" not in user_msg_content


@pytest.mark.asyncio
async def test_review_sends_correct_prompt_structure(mock_ctx, mock_llm):
    await review_selected_file(mock_ctx, mock_llm)

    sent_messages = mock_llm.chat.call_args[0][0]
    assert len(sent_messages) == 2
    assert sent_messages[0].role == "system"
    assert sent_messages[1].role == "user"
    assert "main.py" in sent_messages[1].content


@pytest.mark.asyncio
async def test_review_passes_on_status_callback(mock_ctx, mock_llm):
    await review_selected_file(mock_ctx, mock_llm)

    # on_status should be passed to llm.chat
    call_kwargs = mock_llm.chat.call_args[1]
    assert call_kwargs["on_status"] == mock_ctx.set_status
