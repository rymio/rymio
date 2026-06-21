#RUST litecode-agent by Robert Rymarczyk

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)

MACos, Linux and Windows

A fast, single-binary TUI coding assistant. Browse files, edit code, chat with an LLM, run shell commands, and review proposed patches — all from your terminal.



Built with Rust using [ratatui](https://github.com/ratatui/ratatui) + [crossterm](https://github.com/crossterm-rs/crossterm). No runtime dependencies.

![Layout](https://img.shields.io/badge/TUI-4%20pane%20layout-green)

## Features

- **Coding agent** — Natural language and slash-command interface for code review, error fixing, file creation/editing, and general Q&A, all powered by any OpenAI-compatible LLM
- **RAG-powered context** — Built-in retrieval-augmented generation indexes your project with TF-IDF so the agent can reference code from across the entire codebase, not just the open file
- **File navigation** — Browse your project in a tree view with scrollbar indicators, open and edit files
- **File operations** — Create, edit, navigate, and find files via natural language ("create a Dockerfile") or slash commands (`/create`, `/edit`, `/find`)
- **Code review** — Ask the agent to review your current file for bugs, style, performance, and security issues
- **Error fixing** — Describe an error with a line number or paste a traceback and get a proposed fix as a diff
- **Chat with history** — Ask any coding question with file context; recall previous inputs with Alt+Up/Down, persisted across sessions
- **Search** — Full-text search across your project files
- **Command execution** — Run shell commands, tests, and git diff from within the TUI
- **Patch system** — Review proposed diffs before applying, with automatic `.bak` backups and undo
- **Django utilities** — Find untranslated strings in HTML templates, add date/time headers


---

## Quick Start

```bash
# Install Rust if needed
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/yourusername/litecode-agent.git
cd litecode-agent
cargo build --release

# Run
./target/release/litecode-agent /path/to/your/project
```

Or install directly to your PATH:

```bash
cargo install --path .
litecode-agent
```

---

## Prerequisites

- [Rust toolchain](https://rustup.rs/) (1.70+)
- An LLM server with an OpenAI-compatible `/v1/chat/completions` endpoint (Ollama, llama.cpp, OpenAI, DeepSeek, etc.)

---

## Usage

```bash
# Launch in the current directory
litecode-agent



---

## Configuration

The agent looks for `config.json` in this order:
1. `./config.json` (project root)
2. `~/.config/litecode-agent/config.json` (global fallback)

If neither exists, it defaults to Ollama on localhost.

### Provider Presets

Set `"provider"` and the agent auto-configures the URL and default model:

| Provider | base_url | Default Model |
|----------|----------|---------------|
| `ollama` | `http://127.0.0.1:11434/v1` | `qwen2.5-coder:7b` |
| `llama.cpp` | `http://127.0.0.1:8080/v1` | `local` |
| `openai` | `https://api.openai.com/v1` | `gpt-4o-mini` |
| `deepseek` | `https://api.deepseek.com/v1` | `deepseek-chat` |
| `groq` | `https://api.groq.com/openai/v1` | `llama-3.1-70b-versatile` |
| `together` | `https://api.together.xyz/v1` | `meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo` |
| `openrouter` | `https://openrouter.ai/api/v1` | `meta-llama/llama-3.1-70b-instruct` |

### Example Configs

**Ollama (default, no config needed):**
```json
{
  "provider": "ollama",
  "model": "codellama:13b"
}
```

**OpenAI:**
```json
{
  "provider": "openai",
  "api_key": "sk-...",
  "model": "gpt-4o"
}
```

**DeepSeek:**
```json
{
  "provider": "deepseek",
  "api_key": "sk-..."
}
```

**Custom / self-hosted:**
```json
{
  "base_url": "http://my-server:8000/v1",
  "api_key": "my-key",
  "model": "my-model"
}
```

### All Options

| Option | Default | Description |
|--------|---------|-------------|
| `provider` | `ollama` | Provider preset name |
| `base_url` | *(from provider)* | Override the LLM endpoint URL |
| `api_key` | *(auto)* | API key (`local` for ollama/llama.cpp) |
| `model` | *(from provider)* | Override the model name |
| `max_file_kb` | `300` | Maximum file size (KB) to open |
| `max_prompt_chars` | `30000` | Maximum characters sent to the LLM |
| `temperature` | `0.2` | LLM sampling temperature |
| `max_tokens` | `4096` | Maximum tokens in LLM response |
| `rag_enabled` | `false` | Enable RAG-powered context search |
| `ignored_directories` | `.git`, `.venv`, `node_modules`, etc. | Directories excluded from tree and search |
| `secret_patterns` | `.env`, `*.pem`, `*.key`, etc. | Patterns that trigger a warning before LLM send |
| `test_command` | `python -m pytest` | Command executed by `/test` |

---

## RAG (Retrieval-Augmented Generation)

litecode-agent includes a built-in RAG system that indexes your project and provides relevant context to the LLM when answering questions. This means the agent can reference code from across your project, not just the currently open file.

### How It Works

1. **Tree indexing** — When RAG is enabled, the file tree is indexed so the agent knows what files exist and where they are located
2. **Content chunking** — File contents are split into overlapping chunks (default: 50 lines with 10-line overlap) for granular retrieval
3. **TF-IDF search** — Queries are matched against indexed content using TF-IDF keyword similarity with stopword filtering
4. **Context injection** — The top-k most relevant chunks are prepended to your question before sending to the LLM

### Enabling RAG

Set `"rag_enabled": true` in your config, or toggle it from the Settings page (F10):

```json
{
  "provider": "ollama",
  "rag_enabled": true
}
```

The index is stored at `.litecode/rag_index.json` in your project root and persists across sessions. Files are re-indexed automatically when saved.

### RAG Configuration

| Option | Default | Description |
|--------|---------|-------------|
| `rag_enabled` | `false` | Enable/disable RAG indexing and retrieval |
| `chunk_lines` | `50` | Lines per content chunk |
| `overlap_lines` | `10` | Overlapping lines between chunks |
| `top_k` | `5` | Number of results returned per query |

Binary and compiled files (`.o`, `.so`, `.dll`, `.pyc`, `.wasm`, etc.) are automatically excluded from content indexing.

### Embedding Modes

- **TF-IDF** (default) — Keyword-based similarity using term frequency–inverse document frequency. No external API needed, works entirely offline.
- **API** — Supports OpenAI-compatible embedding endpoints for vector-based semantic search. Requires an embedding model endpoint.

---

## Agent Capabilities

The chat agent understands natural language and slash commands to perform various coding tasks. It uses the currently open file as context and can propose changes as unified diffs.

### Code Intelligence

| Capability | How to Use | What It Does |
|-----------|-----------|--------------|
| **Code review** | `review` | Analyzes the open file for bugs, style issues, performance, and security |
| **Error fixing** | `fix line 42` or paste a traceback | Identifies root cause and proposes a diff fix |
| **General Q&A** | Any question | Answers coding questions with the open file as context |

### File Operations

The agent can create, edit, navigate, and find files — either via slash commands or natural language:

| Operation | Slash Command | Natural Language |
|-----------|--------------|-----------------|
| **Create file** | `/create nginx.conf reverse proxy` | `create a Dockerfile for Python 3.12` |
| **Edit file** | `/edit config.yaml add database section` | `update package.json bump version` |
| **Navigate** | `/open src/main.rs` | `go to config/settings.toml` |
| **Find files** | `/find *.toml` | `where is Cargo.toml` |

When creating files, the agent generates production-quality content with type-specific best practices (Dockerfiles get multi-stage builds, shell scripts get `set -euo pipefail`, nginx configs get security headers, etc.).

File edits are proposed as diffs that require explicit approval before being applied.

### Shell & Testing

| Command | Description |
|---------|-------------|
| `/run <command>` | Execute any shell command with streaming output |
| `/test` | Run the configured test command (default: `python -m pytest`) |
| `/gitdiff` | Show current git diff |
| `/search <term>` | Full-text search across all project files |

### Django Utilities

| Command | Description |
|---------|-------------|
| `translations` | Find untranslated strings in `.html` templates |
| `add date` / `add time` | Add date/time display to template headers |

### Patch Workflow

All code changes proposed by the agent go through a review workflow:

1. Agent proposes a change as a unified diff
2. You review the diff in the Agent Output pane
3. **F5** (or `accept`) applies the patch with automatic `.bak` backup
4. **F6** (or `refuse`) discards the proposal
5. **F7** (or `undo`) reverts the last applied patch from backup

Nothing is written to disk without your explicit approval.

---

## Setting Up a Provider

### Ollama (easiest)

```bash
# Install: https://ollama.com
ollama pull qwen2.5-coder:7b
ollama serve
# Done — litecode-agent works out of the box
```

### llama.cpp

```bash
./llama-server -m your-model.gguf --port 8080
```

Set `"provider": "llama.cpp"` in config.

### Cloud Providers

Set the provider and your API key:
```json
{
  "provider": "openai",
  "api_key": "sk-..."
}
```

Any server implementing `/v1/chat/completions` works.

---

## Keybindings

| Key | Action |
|-----|--------|
| `q` / `Ctrl+Q` | Quit |
| `Tab` | Cycle focus between panes |
| `Ctrl+O` | Open/close full-screen Midnight Commander two-pane file viewer |
| `Ctrl+M` | Commander alias in some terminals; often sent as `Enter` |
| `Ctrl+S` | Save current file (creates .bak backup) |
| `Ctrl+R` | Re-run last shell command |
| `Ctrl+D` | Run `git diff` |
| `F1` | Focus shell |
| `F2` | Focus chat input |
| `F3` | Focus editor |
| `F4` | Create new file |
| `F5` | Accept proposed patch |
| `F6` | Refuse proposed patch |
| `F7` | Undo last applied patch |
| `F8` | Refresh file tree |
| `F9` | About page |
| `F10` | Settings |
| `F11` | Delete file |
| `F12` | Help (all keybindings) |
| `Alt+Up` | Recall previous chat input (history) |
| `Alt+Down` | Recall next chat input (history) |

Inside Commander mode:

| Key | Action |
|-----|--------|
| `Tab` | Switch active left/right pane |
| `Enter` | Open directory or edit the selected file inside commander |
| `F4` | Edit selected file inside commander |
| `Backspace` / `Left` | Go to parent directory |
| `F5` | Copy selected file or directory to the opposite pane |
| `F6` | Move selected file or directory to the opposite pane |
| `Ctrl+S` | Save the file being edited inside commander |
| `Esc` | Close the commander editor |
| `q` / `Ctrl+Q` / `F10` | Return to the main litecode-agent screen |

---

## Chat Commands

Type these in the chat input (F2 to focus):

| Command | Description |
|---------|-------------|
| `review` | Review the currently open file |
| `fix line 42` | Propose a fix for an error at line 42 |
| `translations` | Check Django template for untranslated strings (.html only) |
| `add date` / `add time` | Add date/time to a template header |
| `/search <term>` | Search all project files |
| `/run <command>` | Execute a shell command |
| `/test` | Run the configured test command |
| `/gitdiff` | Run `git diff` |
| `accept` / `apply` | Accept the pending patch |
| `refuse` / `reject` | Discard the pending patch |
| `undo` | Undo the last applied patch |

Anything else is treated as a general coding question about the open file.

---

## Layout

```
┌─────────────────────────────────────────────────────────────┐
│ litecode-agent — filename.rs                                │
├───────────────┬─────────────────────────────────────────────┤
│               │                                             │
│  File Tree    │         Editor (75% w × 75% h)              │
│  (25% w)      │                                             │
│               │                                             │
│               ├─────────────────────────────────────────────┤
│               │  Chat (input + response log)                │
├───────────────┴─────────────────────────────────────────────┤
│ Agent Output (diffs, search results, status)                │
├─────────────────────────────────────────────────────────────┤
│ Shell (command input + streaming output)                    │
├─────────────────────────────────────────────────────────────┤
│ [q Quit] [Tab Next] [F1 Shell] ... [F12 Help]              │
└─────────────────────────────────────────────────────────────┘
```

---

## Building

### Debug build

```bash
cargo build
```

### Release build (optimized)

```bash
cargo build --release
```

### Cross-Compilation

litecode-agent compiles to a single binary for each target platform:

```bash
# Linux (static, no glibc dependency)
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl

# macOS Apple Silicon
rustup target add aarch64-apple-darwin
cargo build --release --target aarch64-apple-darwin

# macOS Intel
rustup target add x86_64-apple-darwin
cargo build --release --target x86_64-apple-darwin

# Windows
rustup target add x86_64-pc-windows-msvc
cargo build --release --target x86_64-pc-windows-msvc
```

The `x86_64-unknown-linux-musl` target is configured to use `zig` from this repository's `.cargo/config.toml`, so make sure `zig` is installed and available on your `PATH` before running the Linux static build.

For targets that need a cross-compiler toolchain, [cross](https://github.com/cross-rs/cross) handles the setup automatically:

```bash
cargo install cross
cross build --release --target aarch64-unknown-linux-gnu
```

---

## Safety

- Patches require explicit approval (F5 or `accept`) — nothing is written automatically
- A `.bak` backup is created before every file write
- Files outside the project root cannot be saved (path traversal prevention)
- Secret files (`.env`, `*.pem`, `*.key`) trigger a warning before content is sent to the LLM
- Dangerous commands (`rm`, `sudo`, `chmod`, etc.) display a warning before execution
- The LLM only sees the currently open file, never the full repository

---

## Running Tests

```bash
cargo test
```

All tests run without network access or external dependencies.

---

## Project Structure

```
src/
├── main.rs          # CLI parsing, terminal setup, app launch
├── app.rs           # App state, event loop, key handling
├── config.rs        # Configuration loading and provider presets
├── history.rs       # Chat history persistence (load/save)
├── llm.rs           # Async HTTP client for LLM APIs
├── tasks.rs         # Task routing (classify user intent)
├── patches.rs       # Diff parsing, validation, apply/undo
├── tools.rs         # File ops, search, command execution
├── prompts.rs       # Prompt templates for LLM interactions
├── file_ops.rs      # File creation and management
├── rag/             # RAG system (chunking, embedding, TF-IDF search)
│   ├── mod.rs
│   ├── config.rs
│   ├── chunker.rs
│   ├── embedding.rs
│   ├── store.rs
│   └── tfidf.rs
└── ui/
    ├── mod.rs       # Layout construction, header/footer
    ├── file_tree.rs # File tree pane with scrollbar
    ├── editor.rs    # Editor pane with line numbers
    ├── chat.rs      # Chat pane with message log and scrollbar
    ├── shell.rs     # Shell pane with streaming output
    ├── agent_output.rs  # Agent output with diff coloring
    ├── settings.rs  # Settings overlay
    └── about.rs     # About page overlay
```

---

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/my-feature`)
3. Make your changes and add tests
4. Run `cargo test` and `cargo clippy` to verify
5. Submit a pull request

---

## License

This project is licensed under the MIT License — see the [LICENSE](LICENSE) file for details.

---

## Author

Robert Rymarczyk — [rob.rym@gmail.com](mailto:rob.rym@gmail.com)
