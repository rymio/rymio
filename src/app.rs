// App struct, event handling, state management

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;

use crate::config::AppConfig;
use crate::file_ops::PendingCreate;
use crate::llm::{LLMClient, LLMConfig};
use crate::patches::PatchSystem;
use crate::prompts::Message;
use crate::rag::SearchHit;
use crate::tasks::{classify, TaskKind};
use crate::tools;

/// The five navigable panes in the TUI.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pane {
    FileTree,
    Editor,
    Chat,
    Shell,
    AgentOutput,
}

impl Pane {
    /// Returns the next pane in the focus cycle order.
    pub fn next(self) -> Self {
        match self {
            Pane::FileTree => Pane::Editor,
            Pane::Editor => Pane::Chat,
            Pane::Chat => Pane::Shell,
            Pane::Shell => Pane::AgentOutput,
            Pane::AgentOutput => Pane::FileTree,
        }
    }
}

/// Messages sent from background async tasks back to the main event loop.
#[allow(dead_code)]
pub enum BackgroundMessage {
    LLMResponse {
        content: String,
        finish_reason: String,
    },
    LLMError(String),
    CommandOutput {
        line: String,
    },
    CommandFinished {
        exit_code: i32,
    },
    RagIndexComplete {
        indexed_count: usize,
    },
    RagIndexError(String),
    RagQueryResult {
        hits: Vec<SearchHit>,
    },
    FileCreateResponse {
        target_path: PathBuf,
        content: String,
    },
    FileEditResponse {
        target_path: PathBuf,
        original_content: String,
        modified_content: String,
    },
}

/// A single entry in the file tree.
pub struct FileEntry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub depth: usize,
}

/// State for the file tree pane.
pub struct FileTreeState {
    pub entries: Vec<FileEntry>,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub expanded_dirs: HashSet<PathBuf>,
    pub visible_height: usize,
}

impl Default for FileTreeState {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            expanded_dirs: HashSet::new(),
            visible_height: 20,
        }
    }
}

/// Which Commander pane is currently active.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CommanderPane {
    Left,
    Right,
}

impl CommanderPane {
    fn other(self) -> Self {
        match self {
            CommanderPane::Left => CommanderPane::Right,
            CommanderPane::Right => CommanderPane::Left,
        }
    }
}

/// State for a single Commander file list pane.
pub struct CommanderListState {
    pub current_dir: PathBuf,
    pub entries: Vec<FileEntry>,
    pub selected_index: usize,
    pub scroll_offset: usize,
}

impl CommanderListState {
    fn new(current_dir: PathBuf) -> Self {
        Self {
            current_dir,
            entries: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
        }
    }
}

/// State for the Commander inline editor.
pub struct CommanderEditorState {
    pub file_path: PathBuf,
    pub content: String,
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub scroll_offset: usize,
}

/// Full-screen two-pane commander state.
pub struct CommanderState {
    pub active_pane: CommanderPane,
    pub left: CommanderListState,
    pub right: CommanderListState,
    pub editor: Option<CommanderEditorState>,
    pub status_message: String,
}

impl CommanderState {
    fn new(start_dir: PathBuf) -> Self {
        Self {
            active_pane: CommanderPane::Left,
            left: CommanderListState::new(start_dir.clone()),
            right: CommanderListState::new(start_dir),
            editor: None,
            status_message: "Tab switches panes. Enter opens. F5 copies. F6 moves.".to_string(),
        }
    }
}

/// State for the editor pane.
pub struct EditorState {
    pub content: String,
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub scroll_offset: usize,
    pub file_path: Option<PathBuf>,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            content: String::new(),
            lines: Vec::new(),
            cursor_row: 0,
            cursor_col: 0,
            scroll_offset: 0,
            file_path: None,
        }
    }
}

/// The role of a chat message participant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatRole {
    User,
    Agent,
    System,
}

/// A single message in the chat log.
#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

/// State for the chat pane.
pub struct ChatState {
    pub input_buffer: String,
    pub cursor_pos: usize,
    pub messages: Vec<ChatMessage>,
    pub scroll_offset: usize,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub history_draft: String,
}

impl Default for ChatState {
    fn default() -> Self {
        Self {
            input_buffer: String::new(),
            cursor_pos: 0,
            messages: Vec::new(),
            scroll_offset: 0,
            history: Vec::new(),
            history_index: None,
            history_draft: String::new(),
        }
    }
}

/// State for the shell pane.
pub struct ShellState {
    pub input_buffer: String,
    pub cursor_pos: usize,
    pub output_lines: Vec<String>,
    pub scroll_offset: usize,
    pub last_command: Option<String>,
    pub is_running: bool,
}

impl Default for ShellState {
    fn default() -> Self {
        Self {
            input_buffer: String::new(),
            cursor_pos: 0,
            output_lines: Vec::new(),
            scroll_offset: 0,
            last_command: None,
            is_running: false,
        }
    }
}

/// Available LLM providers for the settings page.
/// Each entry is (display_name, provider_key).
const SETTINGS_PROVIDERS: &[(&str, &str)] = &[
    ("Ollama (localhost)", "ollama"),
    ("llama.cpp (localhost)", "llama.cpp"),
    ("OpenAI", "openai"),
    ("DeepSeek", "deepseek"),
    ("Groq", "groq"),
    ("Together", "together"),
    ("OpenRouter", "openrouter"),
];

/// State for the settings overlay page.
pub struct SettingsState {
    /// List of (display_name, provider_key) pairs.
    pub providers: Vec<(String, String)>,
    /// Index into `providers` for the currently selected provider.
    pub selected_provider: usize,
    /// Editable base URL buffer.
    pub base_url_buffer: String,
    /// Editable API key buffer.
    pub api_key_buffer: String,
    /// Editable model buffer.
    pub model_buffer: String,
    /// Whether RAG indexing is enabled.
    pub rag_enabled: bool,
    /// Which field is focused: 0=provider, 1=base_url, 2=api_key, 3=model, 4=rag_enabled.
    pub focused_field: usize,
    /// Cursor position within the currently focused text field.
    pub cursor_pos: usize,
}

impl SettingsState {
    /// Create a new SettingsState populated from the current AppConfig.
    pub fn from_config(config: &crate::config::AppConfig) -> Self {
        let providers: Vec<(String, String)> = SETTINGS_PROVIDERS
            .iter()
            .map(|(display, key)| (display.to_string(), key.to_string()))
            .collect();

        // Find the index matching the current provider
        let selected = providers
            .iter()
            .position(|(_, key)| key == &config.provider)
            .unwrap_or(0);

        Self {
            providers,
            selected_provider: selected,
            base_url_buffer: config.base_url.clone(),
            api_key_buffer: config.api_key.clone(),
            model_buffer: config.model.clone(),
            rag_enabled: config.rag_enabled,
            focused_field: 0,
            cursor_pos: 0,
        }
    }

    /// Apply a provider preset, filling in base_url and model defaults.
    pub fn apply_provider_preset(&mut self) {
        let key = &self.providers[self.selected_provider].1;
        if let Some(preset) = crate::config::PROVIDER_PRESETS.get(key.as_str()) {
            self.base_url_buffer = preset.base_url.to_string();
            self.model_buffer = preset.model.to_string();
        }
        self.api_key_buffer = crate::config::default_api_key_for_provider(key).to_string();
    }
}

/// Central application state.
pub struct App {
    pub root_directory: PathBuf,
    pub config: AppConfig,
    pub focus: Pane,
    pub file_tree: FileTreeState,
    pub editor: EditorState,
    pub chat: ChatState,
    pub shell: ShellState,
    pub agent_output: Vec<String>,
    pub patch_system: PatchSystem,
    pub show_about: bool,
    pub show_settings: bool,
    pub settings: SettingsState,
    pub should_quit: bool,
    pub background_tx: mpsc::Sender<BackgroundMessage>,
    pub background_rx: mpsc::Receiver<BackgroundMessage>,
    pub rag_manager: Option<crate::rag::RagManager>,
    /// Whether the "create file" input prompt is active.
    pub create_file_mode: bool,
    /// Buffer for the new file name being typed.
    pub create_file_buffer: String,
    /// Cursor position in the create file buffer.
    pub create_file_cursor: usize,
    /// Whether a delete confirmation prompt is active.
    pub confirm_delete: bool,
    /// Whether the help overlay is shown.
    pub show_help: bool,
    /// Whether an overwrite confirmation is pending for file creation.
    pub confirm_overwrite: bool,
    /// Pending file creation details awaiting overwrite confirmation.
    pub pending_create: Option<PendingCreate>,
    /// Paths from the last find operation for numeric selection.
    pub last_find_results: Vec<PathBuf>,
    /// Whether the Midnight Commander full-screen workspace is active.
    pub commander_mode: bool,
    /// State for the two-pane commander workspace.
    pub commander: CommanderState,
}

impl App {
    /// Create a new App instance with the given configuration and root directory.
    pub fn new(config: AppConfig, root_directory: PathBuf) -> Self {
        let (background_tx, background_rx) = mpsc::channel(100);
        let settings = SettingsState::from_config(&config);
        let commander_start_dir = root_directory.clone();

        let rag_manager = if config.rag_enabled {
            let rag_config = crate::rag::config::RagConfig {
                enabled: true,
                ..Default::default()
            };
            match crate::rag::RagManager::new(rag_config, &root_directory) {
                Ok(manager) => Some(manager),
                Err(e) => {
                    eprintln!("Warning: Failed to initialize RAG: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let mut chat = ChatState::default();
        chat.history = crate::history::load_chat_history();

        Self {
            root_directory,
            config,
            focus: Pane::FileTree,
            file_tree: FileTreeState::default(),
            editor: EditorState::default(),
            chat,
            shell: ShellState::default(),
            agent_output: Vec::new(),
            patch_system: PatchSystem::new(),
            show_about: false,
            show_settings: false,
            settings,
            should_quit: false,
            background_tx,
            background_rx,
            rag_manager,
            create_file_mode: false,
            create_file_buffer: String::new(),
            create_file_cursor: 0,
            confirm_delete: false,
            show_help: false,
            confirm_overwrite: false,
            pending_create: None,
            last_find_results: Vec::new(),
            commander_mode: false,
            commander: CommanderState::new(commander_start_dir),
        }
    }

    /// Run the main event loop.
    /// Multiplexes crossterm terminal events with background task messages.
    pub async fn run(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> anyhow::Result<()> {
        // Load the initial file tree
        self.load_file_tree();

        loop {
            // Render the UI
            terminal.draw(|frame| crate::ui::render(frame, self))?;

            // Drain any pending background messages without blocking. This keeps
            // shell output / LLM responses flowing without starving key input.
            while let Ok(msg) = self.background_rx.try_recv() {
                self.handle_background_message(msg);
            }

            // Drain any pending key events without blocking so fast typing and
            // rapid streaming never lock out the input.
            while event::poll(std::time::Duration::from_millis(0))? {
                if let Event::Key(key) = event::read()? {
                    // Only react to key presses. On many terminals (macOS iTerm2
                    // with kitty protocol, Windows, etc.) crossterm also emits
                    // Release/Repeat events which would otherwise double every
                    // character inserted into the chat and shell input buffers.
                    if key.kind == KeyEventKind::Press {
                        self.handle_key_event(key);
                    }
                }
                if self.should_quit {
                    break;
                }
            }

            if self.should_quit {
                break;
            }

            // Wait for either a new key event or a background message, with a
            // short timeout so we re-render periodically even when idle.
            tokio::select! {
                biased;
                Some(msg) = self.background_rx.recv() => {
                    self.handle_background_message(msg);
                }
                _ = tokio::task::spawn_blocking(|| {
                    // Block until a key event (or other crossterm event) is
                    // available, or the timeout elapses. This wakes the loop
                    // on the very next keystroke instead of the next 50ms tick.
                    let _ = event::poll(std::time::Duration::from_millis(50));
                }) => {}
            }

            if self.should_quit {
                break;
            }
        }

        // Persist RAG store to disk on exit
        if let Some(ref rag_manager) = self.rag_manager {
            if let Err(e) = rag_manager.save() {
                eprintln!("Warning: Failed to save RAG index: {}", e);
            }
        }

        Ok(())
    }

    /// Handle a key event by dispatching to global bindings or pane-specific handlers.
    pub fn handle_key_event(&mut self, key: KeyEvent) {
        // About page dismissal — any key dismisses it
        if self.show_about {
            self.show_about = false;
            return;
        }

        // Help page dismissal — any key dismisses it
        if self.show_help {
            self.show_help = false;
            return;
        }

        // Settings page captures all input when active
        if self.show_settings {
            self.handle_settings_input(key);
            return;
        }

        // Commander mode captures all input while active
        if self.commander_mode {
            self.handle_commander_input(key);
            return;
        }

        // Create file mode captures all input
        if self.create_file_mode {
            self.handle_create_file_input(key);
            return;
        }

        // Delete confirmation mode
        if self.confirm_delete {
            self.handle_confirm_delete_input(key);
            return;
        }

        // Overwrite confirmation mode
        if self.confirm_overwrite {
            self.handle_confirm_overwrite_input(key);
            return;
        }

        // Global key bindings (always active)
        match (key.modifiers, key.code) {
            (KeyModifiers::CONTROL, KeyCode::Char('q')) => {
                self.should_quit = true;
                return;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('s')) => {
                self.handle_save();
                return;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('r')) => {
                self.handle_rerun_command();
                return;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
                self.handle_git_diff();
                return;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('m'))
            | (KeyModifiers::CONTROL, KeyCode::Char('o'))
            | (KeyModifiers::CONTROL, KeyCode::Enter) => {
                self.toggle_commander_mode();
                return;
            }
            (_, KeyCode::Tab) => {
                self.focus = self.focus.next();
                return;
            }
            (_, KeyCode::F(1)) => {
                self.focus = Pane::Shell;
                return;
            }
            (_, KeyCode::F(2)) => {
                self.focus = Pane::Chat;
                return;
            }
            (_, KeyCode::F(3)) => {
                self.focus = Pane::Editor;
                return;
            }
            (_, KeyCode::F(4)) => {
                // Create file: activate input prompt
                self.create_file_mode = true;
                self.create_file_buffer.clear();
                self.create_file_cursor = 0;
                return;
            }
            (_, KeyCode::F(5)) => {
                self.handle_accept_patch();
                return;
            }
            (_, KeyCode::F(6)) => {
                self.handle_refuse_patch();
                return;
            }
            (_, KeyCode::F(7)) => {
                self.handle_undo_patch();
                return;
            }
            (_, KeyCode::F(8)) => {
                self.handle_refresh_tree();
                return;
            }
            (_, KeyCode::F(9)) => {
                self.show_about = !self.show_about;
                return;
            }
            (_, KeyCode::F(10)) => {
                self.show_settings = !self.show_settings;
                if self.show_settings {
                    // Refresh settings state from current config
                    self.settings = SettingsState::from_config(&self.config);
                }
                return;
            }
            (_, KeyCode::F(11)) => {
                // Delete file: activate confirmation prompt
                self.confirm_delete = true;
                return;
            }
            (_, KeyCode::F(12)) => {
                // Help: show all keybindings
                self.show_help = !self.show_help;
                return;
            }
            (_, KeyCode::Esc) => {
                // Escape does nothing when about is not shown and no special mode
                return;
            }
            _ => {}
        }

        // 'q' quits when NOT in Chat, Shell, or Editor input mode
        if key.code == KeyCode::Char('q')
            && key.modifiers == KeyModifiers::NONE
            && self.focus != Pane::Chat
            && self.focus != Pane::Shell
            && self.focus != Pane::Editor
        {
            self.should_quit = true;
            return;
        }

        // Pane-specific handling
        match self.focus {
            Pane::Chat => self.handle_chat_input(key),
            Pane::Shell => self.handle_shell_input(key),
            Pane::FileTree => self.handle_file_tree_input(key),
            Pane::Editor => self.handle_editor_input(key),
            Pane::AgentOutput => {} // Read-only pane
        }
    }

    fn handle_chat_input(&mut self, key: KeyEvent) {
        // Handle Alt+Up/Down for history recall BEFORE plain Up/Down
        if key.modifiers.contains(KeyModifiers::ALT) {
            match key.code {
                KeyCode::Up => {
                    if !self.chat.history.is_empty() {
                        match self.chat.history_index {
                            None => {
                                // Save current input as draft and recall last history entry
                                self.chat.history_draft = self.chat.input_buffer.clone();
                                self.chat.history_index = Some(self.chat.history.len() - 1);
                                self.chat.input_buffer =
                                    self.chat.history[self.chat.history.len() - 1].clone();
                                self.chat.cursor_pos = self.chat.input_buffer.len();
                            }
                            Some(idx) => {
                                if idx > 0 {
                                    self.chat.history_index = Some(idx - 1);
                                    self.chat.input_buffer = self.chat.history[idx - 1].clone();
                                    self.chat.cursor_pos = self.chat.input_buffer.len();
                                }
                            }
                        }
                    }
                    return;
                }
                KeyCode::Down => {
                    if let Some(idx) = self.chat.history_index {
                        if idx + 1 < self.chat.history.len() {
                            self.chat.history_index = Some(idx + 1);
                            self.chat.input_buffer = self.chat.history[idx + 1].clone();
                            self.chat.cursor_pos = self.chat.input_buffer.len();
                        } else {
                            // Past end of history, restore draft
                            self.chat.history_index = None;
                            self.chat.input_buffer = self.chat.history_draft.clone();
                            self.chat.cursor_pos = self.chat.input_buffer.len();
                        }
                    }
                    return;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Enter => {
                let input = self.chat.input_buffer.trim().to_string();
                if !input.is_empty() {
                    // Push to history before clearing
                    self.chat.history.push(input.clone());
                    self.chat.history_index = None;
                    // Persist history to disk
                    let _ = crate::history::save_chat_history(&self.chat.history);
                    self.chat.input_buffer.clear();
                    self.chat.cursor_pos = 0;
                    // Reset scroll to bottom when sending a new message
                    self.chat.scroll_offset = 0;
                    self.submit_chat_input(&input);
                }
            }
            KeyCode::Char(c) => {
                self.chat.input_buffer.insert(self.chat.cursor_pos, c);
                self.chat.cursor_pos += 1;
            }
            KeyCode::Backspace => {
                if self.chat.cursor_pos > 0 {
                    self.chat.cursor_pos -= 1;
                    self.chat.input_buffer.remove(self.chat.cursor_pos);
                }
            }
            KeyCode::Left => {
                if self.chat.cursor_pos > 0 {
                    self.chat.cursor_pos -= 1;
                }
            }
            KeyCode::Right => {
                if self.chat.cursor_pos < self.chat.input_buffer.len() {
                    self.chat.cursor_pos += 1;
                }
            }
            KeyCode::Up => {
                // Scroll message log up
                if self.chat.scroll_offset > 0 {
                    self.chat.scroll_offset -= 1;
                } else {
                    // If at auto-scroll (0), switch to manual scroll from the top of the last visible page
                    self.chat.scroll_offset = usize::MAX; // sentinel: will be clamped in render
                }
            }
            KeyCode::Down => {
                // Scroll message log down (0 means auto-scroll to bottom)
                if self.chat.scroll_offset > 0 {
                    self.chat.scroll_offset -= 1;
                    // If we reach 0, that means auto-scroll to bottom
                }
            }
            KeyCode::PageUp => {
                // Scroll up by a larger amount
                if self.chat.scroll_offset > 5 {
                    self.chat.scroll_offset -= 5;
                } else if self.chat.scroll_offset > 0 {
                    self.chat.scroll_offset = 1;
                } else {
                    self.chat.scroll_offset = usize::MAX;
                }
            }
            KeyCode::PageDown => {
                // Scroll down by a larger amount
                if self.chat.scroll_offset > 5 {
                    self.chat.scroll_offset -= 5;
                } else {
                    self.chat.scroll_offset = 0;
                }
            }
            _ => {}
        }
    }

    fn handle_shell_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                let command = self.shell.input_buffer.trim().to_string();
                if !command.is_empty() {
                    self.shell.input_buffer.clear();
                    self.shell.cursor_pos = 0;
                    self.submit_shell_command(&command);
                }
            }
            KeyCode::Char(c) => {
                self.shell.input_buffer.insert(self.shell.cursor_pos, c);
                self.shell.cursor_pos += 1;
            }
            KeyCode::Backspace => {
                if self.shell.cursor_pos > 0 {
                    self.shell.cursor_pos -= 1;
                    self.shell.input_buffer.remove(self.shell.cursor_pos);
                }
            }
            KeyCode::Left => {
                if self.shell.cursor_pos > 0 {
                    self.shell.cursor_pos -= 1;
                }
            }
            KeyCode::Right => {
                if self.shell.cursor_pos < self.shell.input_buffer.len() {
                    self.shell.cursor_pos += 1;
                }
            }
            _ => {}
        }
    }

    fn handle_file_tree_input(&mut self, key: KeyEvent) {
        let visible_height = self.file_tree.visible_height;
        match key.code {
            KeyCode::Up => {
                if self.file_tree.selected_index > 0 {
                    self.file_tree.selected_index -= 1;
                }
                self.adjust_file_tree_scroll(visible_height);
            }
            KeyCode::Down => {
                if self.file_tree.selected_index + 1 < self.file_tree.entries.len() {
                    self.file_tree.selected_index += 1;
                }
                self.adjust_file_tree_scroll(visible_height);
            }
            KeyCode::Enter => {
                self.handle_file_tree_select();
            }
            KeyCode::Backspace | KeyCode::Left => {
                // Navigate to parent directory (like Midnight Commander)
                self.navigate_to_parent();
            }
            KeyCode::Right => {
                // Expand directory without entering it
                self.handle_file_tree_expand();
            }
            KeyCode::Char('/') => {
                // Jump to filesystem root
                self.root_directory = PathBuf::from("/");
                self.file_tree.expanded_dirs.clear();
                self.file_tree.selected_index = 0;
                self.file_tree.scroll_offset = 0;
                self.load_file_tree();
            }
            KeyCode::Char('~') => {
                // Jump to home directory
                if let Ok(home) = std::env::var("HOME") {
                    self.root_directory = PathBuf::from(home);
                    self.file_tree.expanded_dirs.clear();
                    self.file_tree.selected_index = 0;
                    self.file_tree.scroll_offset = 0;
                    self.load_file_tree();
                }
            }
            _ => {}
        }
    }

    fn handle_editor_input(&mut self, key: KeyEvent) {
        // If the file is empty (e.g. newly created), start with one empty line
        if self.editor.lines.is_empty() {
            self.editor.lines.push(String::new());
        }

        match key.code {
            KeyCode::Char(c) => {
                let line = &mut self.editor.lines[self.editor.cursor_row];
                let col = self.editor.cursor_col.min(line.len());
                line.insert(col, c);
                self.editor.cursor_col = col + 1;
                self.rebuild_content();
            }
            KeyCode::Backspace => {
                if self.editor.cursor_col > 0 {
                    let col = self
                        .editor
                        .cursor_col
                        .min(self.editor.lines[self.editor.cursor_row].len());
                    self.editor.lines[self.editor.cursor_row].remove(col - 1);
                    self.editor.cursor_col = col - 1;
                } else if self.editor.cursor_row > 0 {
                    // Merge with previous line
                    let current_line = self.editor.lines.remove(self.editor.cursor_row);
                    self.editor.cursor_row -= 1;
                    self.editor.cursor_col = self.editor.lines[self.editor.cursor_row].len();
                    self.editor.lines[self.editor.cursor_row].push_str(&current_line);
                }
                self.rebuild_content();
            }
            KeyCode::Delete => {
                let line_len = self.editor.lines[self.editor.cursor_row].len();
                let col = self.editor.cursor_col.min(line_len);
                if col < line_len {
                    self.editor.lines[self.editor.cursor_row].remove(col);
                } else if self.editor.cursor_row + 1 < self.editor.lines.len() {
                    // Merge with next line
                    let next_line = self.editor.lines.remove(self.editor.cursor_row + 1);
                    self.editor.lines[self.editor.cursor_row].push_str(&next_line);
                }
                self.rebuild_content();
            }
            KeyCode::Enter => {
                let line = &self.editor.lines[self.editor.cursor_row];
                let col = self.editor.cursor_col.min(line.len());
                let new_line = line[col..].to_string();
                self.editor.lines[self.editor.cursor_row] = line[..col].to_string();
                self.editor.cursor_row += 1;
                self.editor.lines.insert(self.editor.cursor_row, new_line);
                self.editor.cursor_col = 0;
                self.rebuild_content();
            }
            KeyCode::Up => {
                if self.editor.cursor_row > 0 {
                    self.editor.cursor_row -= 1;
                    self.clamp_cursor_col();
                }
            }
            KeyCode::Down => {
                if self.editor.cursor_row + 1 < self.editor.lines.len() {
                    self.editor.cursor_row += 1;
                    self.clamp_cursor_col();
                }
            }
            KeyCode::Left => {
                if self.editor.cursor_col > 0 {
                    self.editor.cursor_col -= 1;
                }
            }
            KeyCode::Right => {
                let line_len = self.editor.lines[self.editor.cursor_row].len();
                if self.editor.cursor_col < line_len {
                    self.editor.cursor_col += 1;
                }
            }
            KeyCode::Home => {
                self.editor.cursor_col = 0;
            }
            KeyCode::End => {
                self.editor.cursor_col = self.editor.lines[self.editor.cursor_row].len();
            }
            _ => {}
        }

        self.adjust_editor_scroll();
    }

    fn clamp_cursor_col(&mut self) {
        let line_len = self.editor.lines[self.editor.cursor_row].len();
        if self.editor.cursor_col > line_len {
            self.editor.cursor_col = line_len;
        }
    }

    /// Handle key input while the settings overlay is active.
    fn handle_settings_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                // Cancel — close without saving
                self.show_settings = false;
            }
            KeyCode::Enter => {
                // Save settings to config and close
                self.apply_settings();
                self.show_settings = false;
            }
            KeyCode::Tab | KeyCode::Down => {
                // Move to next field
                self.settings.focused_field = (self.settings.focused_field + 1) % 5;
                self.settings.cursor_pos = self.get_settings_field_len();
            }
            KeyCode::Up => {
                // Move to previous field
                self.settings.focused_field = if self.settings.focused_field == 0 {
                    4
                } else {
                    self.settings.focused_field - 1
                };
                self.settings.cursor_pos = self.get_settings_field_len();
            }
            KeyCode::Left => {
                if self.settings.focused_field == 0 {
                    // Cycle provider left
                    let len = self.settings.providers.len();
                    self.settings.selected_provider = if self.settings.selected_provider == 0 {
                        len - 1
                    } else {
                        self.settings.selected_provider - 1
                    };
                    self.settings.apply_provider_preset();
                } else if self.settings.focused_field == 4 {
                    // Toggle RAG enabled
                    self.settings.rag_enabled = !self.settings.rag_enabled;
                } else if self.settings.cursor_pos > 0 {
                    self.settings.cursor_pos -= 1;
                }
            }
            KeyCode::Right => {
                if self.settings.focused_field == 0 {
                    // Cycle provider right
                    let len = self.settings.providers.len();
                    self.settings.selected_provider = (self.settings.selected_provider + 1) % len;
                    self.settings.apply_provider_preset();
                } else if self.settings.focused_field == 4 {
                    // Toggle RAG enabled
                    self.settings.rag_enabled = !self.settings.rag_enabled;
                } else {
                    let field_len = self.get_settings_field_len();
                    if self.settings.cursor_pos < field_len {
                        self.settings.cursor_pos += 1;
                    }
                }
            }
            KeyCode::Char(c) => {
                if self.settings.focused_field > 0 {
                    let pos = self.settings.cursor_pos;
                    match self.settings.focused_field {
                        1 => self.settings.base_url_buffer.insert(pos, c),
                        2 => self.settings.api_key_buffer.insert(pos, c),
                        3 => self.settings.model_buffer.insert(pos, c),
                        _ => {}
                    }
                    self.settings.cursor_pos += 1;
                }
            }
            KeyCode::Backspace => {
                if self.settings.focused_field > 0 && self.settings.cursor_pos > 0 {
                    self.settings.cursor_pos -= 1;
                    let pos = self.settings.cursor_pos;
                    match self.settings.focused_field {
                        1 => {
                            self.settings.base_url_buffer.remove(pos);
                        }
                        2 => {
                            self.settings.api_key_buffer.remove(pos);
                        }
                        3 => {
                            self.settings.model_buffer.remove(pos);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    /// Get the length of the currently focused settings text field.
    fn get_settings_field_len(&self) -> usize {
        match self.settings.focused_field {
            1 => self.settings.base_url_buffer.len(),
            2 => self.settings.api_key_buffer.len(),
            3 => self.settings.model_buffer.len(),
            _ => 0,
        }
    }

    /// Apply settings from the overlay to the live config.
    fn apply_settings(&mut self) {
        let (_, provider_key) = &self.settings.providers[self.settings.selected_provider];
        self.config.provider = provider_key.clone();
        self.config.base_url = self.settings.base_url_buffer.clone();
        self.config.api_key = self.settings.api_key_buffer.clone();
        self.config.model = self.settings.model_buffer.clone();
        self.config.rag_enabled = self.settings.rag_enabled;

        // Handle RAG enable: initialize RagManager and trigger tree scan
        if self.config.rag_enabled && self.rag_manager.is_none() {
            let rag_config = crate::rag::config::RagConfig {
                enabled: true,
                ..Default::default()
            };
            match crate::rag::RagManager::new(rag_config, &self.root_directory) {
                Ok(manager) => {
                    self.rag_manager = Some(manager);
                    self.agent_output
                        .push("RAG: enabled and initialized".to_string());
                    // Trigger initial tree scan
                    self.load_file_tree();
                }
                Err(e) => {
                    self.agent_output
                        .push(format!("RAG: failed to initialize: {}", e));
                    self.config.rag_enabled = false;
                }
            }
        }

        // Handle RAG disable: stop indexing, set rag_manager to None
        if !self.config.rag_enabled && self.rag_manager.is_some() {
            self.rag_manager = None;
            self.agent_output.push("RAG: disabled".to_string());
        }

        self.agent_output.push(format!(
            "Settings updated: provider={}, model={}",
            self.config.provider, self.config.model
        ));

        // Persist settings to config.json
        match crate::config::save_config(&self.root_directory, &self.config) {
            Ok(_) => {
                self.agent_output
                    .push("Settings saved to config.json".to_string());
            }
            Err(e) => {
                self.agent_output.push(format!("Warning: {}", e));
            }
        }
    }

    fn rebuild_content(&mut self) {
        self.editor.content = self.editor.lines.join("\n");
    }

    fn adjust_file_tree_scroll(&mut self, visible_height: usize) {
        if self.file_tree.selected_index < self.file_tree.scroll_offset {
            self.file_tree.scroll_offset = self.file_tree.selected_index;
        } else if self.file_tree.selected_index >= self.file_tree.scroll_offset + visible_height {
            self.file_tree.scroll_offset = self.file_tree.selected_index - visible_height + 1;
        }
    }

    fn adjust_editor_scroll(&mut self) {
        // Keep cursor visible (assume ~20 lines visible as a reasonable default)
        // The actual visible height depends on the terminal size, but we use a safe estimate
        let visible_height = 20;
        if self.editor.cursor_row < self.editor.scroll_offset {
            self.editor.scroll_offset = self.editor.cursor_row;
        } else if self.editor.cursor_row >= self.editor.scroll_offset + visible_height {
            self.editor.scroll_offset = self.editor.cursor_row - visible_height + 1;
        }
    }

    fn toggle_commander_mode(&mut self) {
        self.commander_mode = !self.commander_mode;
        if self.commander_mode {
            let start_dir = self.root_directory.clone();
            self.commander = CommanderState::new(start_dir);
            self.load_commander_entries(CommanderPane::Left);
            self.load_commander_entries(CommanderPane::Right);
        }
    }

    fn handle_commander_input(&mut self, key: KeyEvent) {
        if self.commander.editor.is_some() {
            self.handle_commander_editor_input(key);
            return;
        }

        match (key.modifiers, key.code) {
            (KeyModifiers::CONTROL, KeyCode::Char('q'))
            | (KeyModifiers::CONTROL, KeyCode::Char('o'))
            | (_, KeyCode::F(10)) => {
                self.commander.editor = None;
                self.commander_mode = false;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('m')) => {
                self.commander.editor = None;
                self.commander_mode = false;
            }
            (_, KeyCode::Esc) | (_, KeyCode::Char('q')) => {
                self.commander.editor = None;
                self.commander_mode = false;
            }
            (_, KeyCode::Tab) => {
                self.commander.active_pane = self.commander.active_pane.other();
            }
            (_, KeyCode::Up) => self.move_commander_selection(-1),
            (_, KeyCode::Down) => self.move_commander_selection(1),
            (_, KeyCode::Enter) => self.handle_commander_open(),
            (_, KeyCode::F(4)) => self.handle_commander_edit(),
            (_, KeyCode::Backspace) | (_, KeyCode::Left) => self.navigate_commander_parent(),
            (_, KeyCode::F(5)) => self.copy_commander_selection(),
            (_, KeyCode::F(6)) => self.move_commander_selection_to_other_pane(),
            _ => {}
        }
    }

    fn handle_commander_editor_input(&mut self, key: KeyEvent) {
        match (key.modifiers, key.code) {
            (KeyModifiers::CONTROL, KeyCode::Char('q'))
            | (KeyModifiers::CONTROL, KeyCode::Char('o'))
            | (_, KeyCode::F(10)) => {
                self.commander.editor = None;
                self.commander_mode = false;
                return;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('s')) => {
                self.save_commander_editor();
                return;
            }
            (_, KeyCode::Esc) => {
                self.commander.editor = None;
                self.commander.status_message = "Editor closed.".to_string();
                return;
            }
            _ => {}
        }

        let Some(editor) = self.commander.editor.as_mut() else {
            return;
        };

        if editor.lines.is_empty() {
            editor.lines.push(String::new());
        }

        match key.code {
            KeyCode::Char(c) => {
                let line = &mut editor.lines[editor.cursor_row];
                let col = editor.cursor_col.min(line.len());
                line.insert(col, c);
                editor.cursor_col = col + 1;
            }
            KeyCode::Backspace => {
                if editor.cursor_col > 0 {
                    let col = editor.cursor_col.min(editor.lines[editor.cursor_row].len());
                    editor.lines[editor.cursor_row].remove(col - 1);
                    editor.cursor_col = col - 1;
                } else if editor.cursor_row > 0 {
                    let current_line = editor.lines.remove(editor.cursor_row);
                    editor.cursor_row -= 1;
                    editor.cursor_col = editor.lines[editor.cursor_row].len();
                    editor.lines[editor.cursor_row].push_str(&current_line);
                }
            }
            KeyCode::Delete => {
                let line_len = editor.lines[editor.cursor_row].len();
                let col = editor.cursor_col.min(line_len);
                if col < line_len {
                    editor.lines[editor.cursor_row].remove(col);
                } else if editor.cursor_row + 1 < editor.lines.len() {
                    let next_line = editor.lines.remove(editor.cursor_row + 1);
                    editor.lines[editor.cursor_row].push_str(&next_line);
                }
            }
            KeyCode::Enter => {
                let line = &editor.lines[editor.cursor_row];
                let col = editor.cursor_col.min(line.len());
                let new_line = line[col..].to_string();
                editor.lines[editor.cursor_row] = line[..col].to_string();
                editor.cursor_row += 1;
                editor.lines.insert(editor.cursor_row, new_line);
                editor.cursor_col = 0;
            }
            KeyCode::Up => {
                if editor.cursor_row > 0 {
                    editor.cursor_row -= 1;
                    let line_len = editor.lines[editor.cursor_row].len();
                    editor.cursor_col = editor.cursor_col.min(line_len);
                }
            }
            KeyCode::Down => {
                if editor.cursor_row + 1 < editor.lines.len() {
                    editor.cursor_row += 1;
                    let line_len = editor.lines[editor.cursor_row].len();
                    editor.cursor_col = editor.cursor_col.min(line_len);
                }
            }
            KeyCode::Left => {
                if editor.cursor_col > 0 {
                    editor.cursor_col -= 1;
                }
            }
            KeyCode::Right => {
                let line_len = editor.lines[editor.cursor_row].len();
                if editor.cursor_col < line_len {
                    editor.cursor_col += 1;
                }
            }
            KeyCode::Home => editor.cursor_col = 0,
            KeyCode::End => editor.cursor_col = editor.lines[editor.cursor_row].len(),
            _ => {}
        }

        editor.content = editor.lines.join("\n");
        self.adjust_commander_editor_scroll();
    }

    fn adjust_commander_editor_scroll(&mut self) {
        let Some(editor) = self.commander.editor.as_mut() else {
            return;
        };

        let visible_height = 20;
        if editor.cursor_row < editor.scroll_offset {
            editor.scroll_offset = editor.cursor_row;
        } else if editor.cursor_row >= editor.scroll_offset + visible_height {
            editor.scroll_offset = editor.cursor_row - visible_height + 1;
        }
    }

    fn active_commander_pane(&self) -> &CommanderListState {
        match self.commander.active_pane {
            CommanderPane::Left => &self.commander.left,
            CommanderPane::Right => &self.commander.right,
        }
    }

    fn active_commander_pane_mut(&mut self) -> &mut CommanderListState {
        match self.commander.active_pane {
            CommanderPane::Left => &mut self.commander.left,
            CommanderPane::Right => &mut self.commander.right,
        }
    }

    fn inactive_commander_pane_mut(&mut self) -> &mut CommanderListState {
        match self.commander.active_pane {
            CommanderPane::Left => &mut self.commander.right,
            CommanderPane::Right => &mut self.commander.left,
        }
    }

    fn load_commander_entries(&mut self, pane: CommanderPane) {
        let current_dir = match pane {
            CommanderPane::Left => self.commander.left.current_dir.clone(),
            CommanderPane::Right => self.commander.right.current_dir.clone(),
        };

        let mut entries = Vec::new();
        if current_dir.parent().is_some() {
            entries.push(FileEntry {
                path: PathBuf::from(".."),
                name: "..".to_string(),
                is_dir: true,
                depth: 0,
            });
        }

        if let Ok(read_dir) = fs::read_dir(&current_dir) {
            let mut dirs = Vec::new();
            let mut files = Vec::new();
            for entry in read_dir.flatten() {
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                let name = entry.file_name().to_string_lossy().to_string();
                if file_type.is_dir() {
                    dirs.push((name, entry.path()));
                } else {
                    files.push((name, entry.path()));
                }
            }
            dirs.sort_by(|a, b| a.0.cmp(&b.0));
            files.sort_by(|a, b| a.0.cmp(&b.0));

            for (name, path) in dirs {
                entries.push(FileEntry {
                    path,
                    name,
                    is_dir: true,
                    depth: 0,
                });
            }
            for (name, path) in files {
                entries.push(FileEntry {
                    path,
                    name,
                    is_dir: false,
                    depth: 0,
                });
            }
        }

        let pane_state = match pane {
            CommanderPane::Left => &mut self.commander.left,
            CommanderPane::Right => &mut self.commander.right,
        };
        pane_state.entries = entries;
        if pane_state.selected_index >= pane_state.entries.len() {
            pane_state.selected_index = pane_state.entries.len().saturating_sub(1);
        }
        if pane_state.scroll_offset > pane_state.selected_index {
            pane_state.scroll_offset = pane_state.selected_index;
        }
    }

    fn move_commander_selection(&mut self, delta: isize) {
        let pane = self.active_commander_pane_mut();
        if pane.entries.is_empty() {
            return;
        }

        let max_index = pane.entries.len().saturating_sub(1) as isize;
        let next = (pane.selected_index as isize + delta).clamp(0, max_index) as usize;
        pane.selected_index = next;
        let visible_height = 18usize;
        if pane.selected_index < pane.scroll_offset {
            pane.scroll_offset = pane.selected_index;
        } else if pane.selected_index >= pane.scroll_offset + visible_height {
            pane.scroll_offset = pane.selected_index - visible_height + 1;
        }
    }

    fn handle_commander_open(&mut self) {
        let Some(entry) = self
            .active_commander_pane()
            .entries
            .get(self.active_commander_pane().selected_index)
            .map(|entry| (entry.path.clone(), entry.name.clone(), entry.is_dir))
        else {
            return;
        };

        let (entry_path, entry_name, entry_is_dir) = entry;

        if entry_is_dir {
            if entry_name == ".." {
                self.navigate_commander_parent();
                return;
            }

            let active = self.commander.active_pane;
            {
                let pane = self.active_commander_pane_mut();
                pane.current_dir = entry_path.clone();
                pane.selected_index = 0;
                pane.scroll_offset = 0;
            }
            self.load_commander_entries(active);
            self.commander.status_message = format!("Opened {}", entry_path.display());
            return;
        }

        self.open_file_in_commander_editor(&entry_path);
    }

    fn handle_commander_edit(&mut self) {
        let Some(entry) = self
            .active_commander_pane()
            .entries
            .get(self.active_commander_pane().selected_index)
            .map(|entry| (entry.path.clone(), entry.is_dir))
        else {
            return;
        };

        if entry.1 {
            self.commander.status_message = "Select a file to edit.".to_string();
            return;
        }

        self.open_file_in_commander_editor(&entry.0);
    }

    fn navigate_commander_parent(&mut self) {
        let active = self.commander.active_pane;
        let parent = self
            .active_commander_pane()
            .current_dir
            .parent()
            .map(|path| path.to_path_buf());

        if let Some(parent) = parent {
            {
                let pane = self.active_commander_pane_mut();
                pane.current_dir = parent.clone();
                pane.selected_index = 0;
                pane.scroll_offset = 0;
            }
            self.load_commander_entries(active);
            self.commander.status_message = format!("Opened {}", parent.display());
        }
    }

    fn copy_commander_selection(&mut self) {
        let (source_path, source_name) = match self
            .active_commander_pane()
            .entries
            .get(self.active_commander_pane().selected_index)
        {
            Some(entry) if entry.name != ".." => (entry.path.clone(), entry.name.clone()),
            _ => {
                self.commander.status_message = "Nothing selected to copy.".to_string();
                return;
            }
        };

        let destination_dir = self.inactive_commander_pane_mut().current_dir.clone();
        let destination_path = destination_dir.join(&source_name);

        match copy_path_recursive(&source_path, &destination_path) {
            Ok(()) => {
                self.commander.status_message =
                    format!("Copied {} to {}", source_name, destination_dir.display());
                self.load_commander_entries(self.commander.active_pane.other());
            }
            Err(err) => {
                self.commander.status_message = format!("Copy failed: {err}");
            }
        }
    }

    fn move_commander_selection_to_other_pane(&mut self) {
        let (source_path, source_name) = match self
            .active_commander_pane()
            .entries
            .get(self.active_commander_pane().selected_index)
        {
            Some(entry) if entry.name != ".." => (entry.path.clone(), entry.name.clone()),
            _ => {
                self.commander.status_message = "Nothing selected to move.".to_string();
                return;
            }
        };

        let destination_dir = self.inactive_commander_pane_mut().current_dir.clone();
        let destination_path = destination_dir.join(&source_name);

        let move_result = fs::rename(&source_path, &destination_path).or_else(|_| {
            copy_path_recursive(&source_path, &destination_path)?;
            remove_path_recursive(&source_path)
        });

        match move_result {
            Ok(()) => {
                self.commander.status_message =
                    format!("Moved {} to {}", source_name, destination_dir.display());
                let active = self.commander.active_pane;
                self.load_commander_entries(active);
                self.load_commander_entries(active.other());
            }
            Err(err) => {
                self.commander.status_message = format!("Move failed: {err}");
            }
        }
    }

    fn open_file_in_commander_editor(&mut self, path: &Path) {
        match crate::tools::read_file_safe(path, self.config.max_file_kb) {
            Ok(content) => {
                let lines = if content.is_empty() {
                    vec![String::new()]
                } else {
                    content.lines().map(String::from).collect()
                };
                self.commander.editor = Some(CommanderEditorState {
                    file_path: path.to_path_buf(),
                    content,
                    lines,
                    cursor_row: 0,
                    cursor_col: 0,
                    scroll_offset: 0,
                });
                self.commander.status_message =
                    format!("Editing {} (Ctrl+S to save, Esc to close)", path.display());
            }
            Err(err) => {
                self.commander.status_message = err;
            }
        }
    }

    fn save_commander_editor(&mut self) {
        let Some(editor) = self.commander.editor.as_mut() else {
            return;
        };

        editor.content = editor.lines.join("\n");
        match fs::write(&editor.file_path, &editor.content) {
            Ok(()) => {
                self.commander.status_message = format!("Saved {}", editor.file_path.display());
            }
            Err(err) => {
                self.commander.status_message = format!("Save failed: {err}");
            }
        }
    }

    // --- Action handlers (placeholders for later wiring) ---

    fn handle_save(&mut self) {
        let Some(ref file_path) = self.editor.file_path else {
            self.agent_output.push("No file open to save.".to_string());
            return;
        };

        // Clone file_path to avoid borrow conflict with self.rag_manager later
        let file_path = file_path.clone();
        let full_path = self.root_directory.join(&file_path);

        // Create .bak backup
        let backup_path = full_path.with_extension(format!(
            "{}.bak",
            full_path
                .extension()
                .unwrap_or_default()
                .to_str()
                .unwrap_or("")
        ));

        if full_path.exists() {
            if let Err(e) = std::fs::copy(&full_path, &backup_path) {
                self.agent_output
                    .push(format!("Warning: Failed to create backup: {e}"));
            }
        }

        // Write content
        match std::fs::write(&full_path, &self.editor.content) {
            Ok(_) => {
                self.agent_output
                    .push(format!("Saved: {}", full_path.display()));

                // Re-index file content in RAG if enabled and previously indexed
                if let Some(ref mut rag_manager) = self.rag_manager {
                    if rag_manager.is_enabled() {
                        // Use full absolute path for RAG indexing
                        let indexed = rag_manager.index_file_content(
                            &full_path,
                            &self.editor.content,
                            self.config.max_file_kb,
                        );
                        if indexed > 0 {
                            self.agent_output.push(format!(
                                "RAG: re-indexed {} content chunks for {}",
                                indexed,
                                full_path.display()
                            ));
                            // Persist RAG store after re-indexing
                            let _ = rag_manager.save();
                        }
                    }
                }
            }
            Err(e) => {
                self.agent_output.push(format!("Error saving file: {e}"));
            }
        }
    }

    fn handle_rerun_command(&mut self) {
        if let Some(cmd) = self.shell.last_command.clone() {
            self.submit_shell_command(&cmd);
        }
    }

    fn handle_git_diff(&mut self) {
        self.submit_shell_command("git diff");
    }

    fn handle_accept_patch(&mut self) {
        match self.patch_system.apply_patch() {
            Ok(msg) => {
                self.agent_output.push(msg);
                // Reload editor if the patched file is currently open
                if let Some(ref proposal) = self.patch_system.last_applied() {
                    let target_relative = proposal
                        .target_file
                        .strip_prefix(&self.root_directory)
                        .map(|r| r.to_path_buf())
                        .unwrap_or_else(|_| proposal.target_file.clone());
                    if self.editor.file_path.as_ref() == Some(&target_relative) {
                        let full_path = self.root_directory.join(&target_relative);
                        if let Ok(content) =
                            crate::tools::read_file_safe(&full_path, self.config.max_file_kb)
                        {
                            self.editor.lines = content.lines().map(String::from).collect();
                            self.editor.content = content;
                            // Preserve cursor position if possible
                            let max_row = self.editor.lines.len().saturating_sub(1);
                            if self.editor.cursor_row > max_row {
                                self.editor.cursor_row = max_row;
                            }
                        }
                    }
                }
            }
            Err(msg) => self.agent_output.push(msg),
        }
    }

    fn handle_refuse_patch(&mut self) {
        let msg = self.patch_system.refuse_patch();
        self.agent_output.push(msg);
    }

    fn handle_undo_patch(&mut self) {
        // Get the target path before undo takes the proposal
        let target_relative = self.patch_system.last_applied().map(|p| {
            p.target_file
                .strip_prefix(&self.root_directory)
                .map(|r| r.to_path_buf())
                .unwrap_or_else(|_| p.target_file.clone())
        });

        match self.patch_system.undo_last_patch() {
            Ok(msg) => {
                self.agent_output.push(msg);
                // Reload editor if the undone file is currently open
                if let Some(ref target) = target_relative {
                    if self.editor.file_path.as_ref() == Some(target) {
                        let full_path = self.root_directory.join(target);
                        if let Ok(content) =
                            crate::tools::read_file_safe(&full_path, self.config.max_file_kb)
                        {
                            self.editor.lines = content.lines().map(String::from).collect();
                            self.editor.content = content;
                            // Preserve cursor position if possible
                            let max_row = self.editor.lines.len().saturating_sub(1);
                            if self.editor.cursor_row > max_row {
                                self.editor.cursor_row = max_row;
                            }
                        }
                    }
                }
            }
            Err(msg) => self.agent_output.push(msg),
        }
    }

    /// Handle input while the "create file" prompt is active.
    fn handle_create_file_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                // Cancel create file
                self.create_file_mode = false;
                self.create_file_buffer.clear();
                self.create_file_cursor = 0;
            }
            KeyCode::Enter => {
                // Confirm create file
                let name = self.create_file_buffer.trim().to_string();
                self.create_file_mode = false;
                self.create_file_buffer.clear();
                self.create_file_cursor = 0;

                if name.is_empty() {
                    self.agent_output
                        .push("Create file cancelled: empty name.".to_string());
                    return;
                }

                self.execute_create_file(&name);
            }
            KeyCode::Char(c) => {
                self.create_file_buffer.insert(self.create_file_cursor, c);
                self.create_file_cursor += 1;
            }
            KeyCode::Backspace => {
                if self.create_file_cursor > 0 {
                    self.create_file_cursor -= 1;
                    self.create_file_buffer.remove(self.create_file_cursor);
                }
            }
            KeyCode::Left => {
                if self.create_file_cursor > 0 {
                    self.create_file_cursor -= 1;
                }
            }
            KeyCode::Right => {
                if self.create_file_cursor < self.create_file_buffer.len() {
                    self.create_file_cursor += 1;
                }
            }
            _ => {}
        }
    }

    /// Actually create the file on disk.
    fn execute_create_file(&mut self, name: &str) {
        let full_path = self.root_directory.join(name);

        // If the name ends with '/', create a directory
        if name.ends_with('/') {
            match fs::create_dir_all(&full_path) {
                Ok(_) => {
                    self.agent_output
                        .push(format!("Created directory: {}", name));
                    self.load_file_tree();
                }
                Err(e) => {
                    self.agent_output
                        .push(format!("Error creating directory: {e}"));
                }
            }
        } else {
            // Create parent directories if needed
            if let Some(parent) = full_path.parent() {
                if !parent.exists() {
                    if let Err(e) = fs::create_dir_all(parent) {
                        self.agent_output
                            .push(format!("Error creating parent dirs: {e}"));
                        return;
                    }
                }
            }

            // Don't overwrite existing files
            if full_path.exists() {
                self.agent_output
                    .push(format!("File already exists: {}", name));
                return;
            }

            match fs::write(&full_path, "") {
                Ok(_) => {
                    self.agent_output.push(format!("Created file: {}", name));
                    self.load_file_tree();
                }
                Err(e) => {
                    self.agent_output.push(format!("Error creating file: {e}"));
                }
            }
        }
    }

    /// Handle input while the delete confirmation prompt is active.
    fn handle_confirm_delete_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.confirm_delete = false;
                self.execute_delete_file();
            }
            _ => {
                // Any other key cancels
                self.confirm_delete = false;
                self.agent_output.push("Delete cancelled.".to_string());
            }
        }
    }

    /// Handle key input while the overwrite confirmation prompt is active.
    fn handle_confirm_overwrite_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.confirm_overwrite = false;
                if let Some(pending) = self.pending_create.take() {
                    // Spawn LLM task to generate content for the overwrite
                    let filename = pending
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    self.spawn_file_create_task(&filename, "", pending.path);
                }
            }
            _ => {
                // Any other key cancels
                self.confirm_overwrite = false;
                self.pending_create = None;
                self.chat.messages.push(ChatMessage {
                    role: ChatRole::System,
                    content: "File creation cancelled.".to_string(),
                });
            }
        }
    }

    /// Delete the currently selected file/directory in the file tree.
    fn execute_delete_file(&mut self) {
        let Some(entry) = self.file_tree.entries.get(self.file_tree.selected_index) else {
            self.agent_output.push("No file selected.".to_string());
            return;
        };

        // Don't allow deleting the ".." entry
        if entry.name == ".." {
            self.agent_output
                .push("Cannot delete parent directory entry.".to_string());
            return;
        }

        let path = entry.path.clone();
        let is_dir = entry.is_dir;
        let full_path = self.root_directory.join(&path);

        if is_dir {
            match fs::remove_dir_all(&full_path) {
                Ok(_) => {
                    self.agent_output
                        .push(format!("Deleted directory: {}", path.display()));
                    self.load_file_tree();
                }
                Err(e) => {
                    self.agent_output
                        .push(format!("Error deleting directory: {e}"));
                }
            }
        } else {
            match fs::remove_file(&full_path) {
                Ok(_) => {
                    self.agent_output
                        .push(format!("Deleted file: {}", path.display()));
                    // If the deleted file was open in the editor, clear it
                    if self.editor.file_path.as_ref() == Some(&path) {
                        self.editor.file_path = None;
                        self.editor.content.clear();
                        self.editor.lines.clear();
                        self.editor.cursor_row = 0;
                        self.editor.cursor_col = 0;
                    }
                    self.load_file_tree();
                }
                Err(e) => {
                    self.agent_output.push(format!("Error deleting file: {e}"));
                }
            }
        }
    }

    /// Load the file tree from the root directory, building a flat list of entries
    /// that respects expanded directories and filters ignored directories.
    pub fn load_file_tree(&mut self) {
        let mut entries = Vec::new();

        // Add ".." entry at the top for parent navigation (like Midnight Commander)
        if self.root_directory.parent().is_some() {
            entries.push(FileEntry {
                path: PathBuf::from(".."),
                name: "..".to_string(),
                is_dir: true,
                depth: 0,
            });
        }

        self.build_tree_entries(&self.root_directory.clone(), 0, &mut entries);
        self.file_tree.entries = entries;

        // Index tree entries in RAG if enabled
        if let Some(ref mut rag_manager) = self.rag_manager {
            if rag_manager.is_enabled() {
                let root = self.root_directory.clone();
                let tree_entries: Vec<crate::rag::TreeEntry> = self
                    .file_tree
                    .entries
                    .iter()
                    .filter(|e| e.name != "..")
                    .map(|e| crate::rag::TreeEntry {
                        // Use full absolute path so RAG always knows the real location
                        path: root.join(&e.path),
                        is_directory: e.is_dir,
                    })
                    .collect();
                let ignored_dirs = self.config.ignored_directories.clone();
                let indexed = rag_manager.index_tree_entries(&tree_entries, &ignored_dirs);
                if indexed > 0 {
                    self.agent_output
                        .push(format!("RAG: indexed {} tree entries", indexed));
                    // Persist RAG store after indexing
                    let _ = rag_manager.save();
                }
            }
        }

        // Clamp selected_index if tree shrunk
        if self.file_tree.selected_index >= self.file_tree.entries.len()
            && !self.file_tree.entries.is_empty()
        {
            self.file_tree.selected_index = self.file_tree.entries.len() - 1;
        }
    }

    /// Recursively build the flat entry list for the file tree.
    /// Directories are sorted before files, and both groups are sorted alphabetically.
    /// Only children of expanded directories are included.
    fn build_tree_entries(&self, dir: &Path, depth: usize, entries: &mut Vec<FileEntry>) {
        let read_dir = match fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(_) => return,
        };

        let mut dirs: Vec<(String, PathBuf)> = Vec::new();
        let mut files: Vec<(String, PathBuf)> = Vec::new();

        for entry in read_dir.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().to_string();

            if file_type.is_dir() {
                // Filter ignored directories
                if self.config.ignored_directories.contains(&name) {
                    continue;
                }
                dirs.push((name, entry.path()));
            } else {
                files.push((name, entry.path()));
            }
        }

        // Sort alphabetically (case-sensitive)
        dirs.sort_by(|a, b| a.0.cmp(&b.0));
        files.sort_by(|a, b| a.0.cmp(&b.0));

        // Add directories first
        for (name, full_path) in dirs {
            let relative_path = full_path
                .strip_prefix(&self.root_directory)
                .unwrap_or(&full_path)
                .to_path_buf();

            entries.push(FileEntry {
                path: relative_path.clone(),
                name,
                is_dir: true,
                depth,
            });

            // If this directory is expanded, recurse into it
            if self.file_tree.expanded_dirs.contains(&relative_path) {
                self.build_tree_entries(&full_path, depth + 1, entries);
            }
        }

        // Add files
        for (name, full_path) in files {
            let relative_path = full_path
                .strip_prefix(&self.root_directory)
                .unwrap_or(&full_path)
                .to_path_buf();

            entries.push(FileEntry {
                path: relative_path,
                name,
                is_dir: false,
                depth,
            });
        }
    }

    fn handle_refresh_tree(&mut self) {
        self.load_file_tree();

        // Reconcile RAG tree index with current filesystem state
        if let Some(ref mut rag_manager) = self.rag_manager {
            if rag_manager.is_enabled() {
                let root = self.root_directory.clone();
                let current_paths: HashSet<PathBuf> = self
                    .file_tree
                    .entries
                    .iter()
                    .filter(|e| e.name != "..")
                    .map(|e| root.join(&e.path))
                    .collect();
                rag_manager.reconcile_tree(&current_paths);
                // Persist RAG store after reconciliation
                let _ = rag_manager.save();
            }
        }
    }

    fn handle_file_tree_select(&mut self) {
        let Some(entry) = self.file_tree.entries.get(self.file_tree.selected_index) else {
            return;
        };

        let path = entry.path.clone();
        let is_dir = entry.is_dir;

        if is_dir {
            if path == PathBuf::from("..") {
                // Go to parent directory
                self.navigate_to_parent();
            } else {
                // Enter the directory (change root — like Midnight Commander)
                let full_path = self.root_directory.join(&path);
                if full_path.is_dir() {
                    self.root_directory = full_path;
                    self.file_tree.expanded_dirs.clear();
                    self.file_tree.selected_index = 0;
                    self.file_tree.scroll_offset = 0;
                    self.load_file_tree();
                }
            }
        } else {
            // Open file in editor
            let full_path = self.root_directory.join(&path);
            match crate::tools::read_file_safe(&full_path, self.config.max_file_kb) {
                Ok(content) => {
                    self.editor.lines = content.lines().map(String::from).collect();
                    self.editor.content = content;
                    self.editor.file_path = Some(path.clone());
                    self.editor.cursor_row = 0;
                    self.editor.cursor_col = 0;
                    self.editor.scroll_offset = 0;

                    // Index file content in RAG if enabled
                    if let Some(ref mut rag_manager) = self.rag_manager {
                        if rag_manager.is_enabled() {
                            // Use full absolute path for RAG indexing
                            let rag_path = self.root_directory.join(&path);
                            let indexed = rag_manager.index_file_content(
                                &rag_path,
                                &self.editor.content,
                                self.config.max_file_kb,
                            );
                            if indexed > 0 {
                                self.agent_output.push(format!(
                                    "RAG: indexed {} content chunks for {}",
                                    indexed,
                                    rag_path.display()
                                ));
                                // Persist RAG store after content indexing
                                let _ = rag_manager.save();
                            }
                        }
                    }
                }
                Err(err) => {
                    self.agent_output.push(err);
                }
            }
        }
    }

    /// Expand/collapse a directory in-place (Right arrow) without entering it.
    fn handle_file_tree_expand(&mut self) {
        let Some(entry) = self.file_tree.entries.get(self.file_tree.selected_index) else {
            return;
        };

        if !entry.is_dir {
            return;
        }

        let path = entry.path.clone();
        if self.file_tree.expanded_dirs.contains(&path) {
            self.file_tree.expanded_dirs.remove(&path);
        } else {
            self.file_tree.expanded_dirs.insert(path);
        }
        self.load_file_tree();
    }

    /// Navigate to the parent directory, allowing traversal above the initial project root.
    fn navigate_to_parent(&mut self) {
        if let Some(parent) = self.root_directory.parent() {
            let parent = parent.to_path_buf();
            if parent.is_dir() {
                self.root_directory = parent;
                self.file_tree.expanded_dirs.clear();
                self.file_tree.selected_index = 0;
                self.file_tree.scroll_offset = 0;
                self.load_file_tree();
                self.agent_output
                    .push(format!("Navigated to: {}", self.root_directory.display()));
            }
        }
    }

    fn submit_chat_input(&mut self, input: &str) {
        // Add user message to chat
        self.chat.messages.push(ChatMessage {
            role: ChatRole::User,
            content: input.to_string(),
        });

        // Check for text commands
        let lower = input.to_lowercase();
        match lower.as_str() {
            "accept" | "apply" => {
                self.handle_accept_patch();
                return;
            }
            "refuse" | "reject" => {
                self.handle_refuse_patch();
                return;
            }
            "undo" => {
                self.handle_undo_patch();
                return;
            }
            _ => {}
        }

        // Check if input is a numeric selection from find results
        if let Ok(num) = input.trim().parse::<usize>() {
            if !self.last_find_results.is_empty() {
                if num >= 1 && num <= self.last_find_results.len() {
                    let path = self.last_find_results[num - 1]
                        .to_string_lossy()
                        .to_string();
                    let result = crate::file_ops::handle_navigate(&path, &self.root_directory);
                    self.apply_file_op_result(result);
                    self.last_find_results.clear();
                    return;
                }
            }
        }

        // Classify and dispatch the task
        let file_ext = self.get_file_ext().map(|s| s.to_string());
        let task = classify(input, file_ext.as_deref());
        self.dispatch_task(task);
    }

    /// Get the file extension of the currently open file in the editor.
    fn get_file_ext(&self) -> Option<&str> {
        self.editor
            .file_path
            .as_ref()
            .and_then(|p| p.extension())
            .and_then(|ext| ext.to_str())
    }

    /// Get the filename of the currently open file for use in prompts.
    fn get_current_filename(&self) -> String {
        self.editor
            .file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("untitled")
            .to_string()
    }

    /// Extract a ±40 line snippet around the target line, with line numbers.
    fn extract_snippet(&self, target_line: u32) -> String {
        let lines = &self.editor.lines;
        if lines.is_empty() {
            return String::new();
        }

        let target = target_line as usize;
        let start = target.saturating_sub(41);
        let end = (target + 40).min(lines.len());

        lines[start..end]
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{:>4} | {}", start + i + 1, line))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Main dispatch logic: route a classified task to the appropriate handler.
    fn dispatch_task(&mut self, task: TaskKind) {
        match task {
            TaskKind::Review => {
                self.handle_review_task();
            }
            TaskKind::FixError {
                line_number,
                error_text,
            } => {
                self.handle_fix_error_task(line_number, error_text);
            }
            TaskKind::Search { term } => {
                self.handle_search_task(&term);
            }
            TaskKind::RunCommand { command } => {
                self.submit_shell_command(&command);
            }
            TaskKind::RunTest => {
                let test_cmd = self.config.test_command.clone();
                self.submit_shell_command(&test_cmd);
            }
            TaskKind::GitDiff => {
                self.submit_shell_command("git diff");
            }
            TaskKind::TranslationCheck => {
                self.handle_translation_check_task();
            }
            TaskKind::HeaderDateTime => {
                self.handle_header_datetime_task();
            }
            TaskKind::FileNavigate { path } => {
                let result = crate::file_ops::handle_navigate(&path, &self.root_directory);
                self.apply_file_op_result(result);
            }
            TaskKind::FileCreate {
                filename,
                description,
            } => {
                // Determine the currently selected directory
                let selected_dir = self.get_selected_directory();
                let (result, pending) = crate::file_ops::handle_create_start(
                    &filename,
                    &description,
                    &selected_dir,
                    &self.root_directory,
                );

                if let Some(pending_create) = pending {
                    // File already exists — ask for overwrite confirmation
                    self.confirm_overwrite = true;
                    self.pending_create = Some(pending_create);
                    self.chat.messages.push(ChatMessage {
                        role: ChatRole::System,
                        content: result.message,
                    });
                } else if result.success {
                    // Spawn LLM task for content generation
                    self.chat.messages.push(ChatMessage {
                        role: ChatRole::System,
                        content: result.message,
                    });
                    let target_path = selected_dir.join(&filename);
                    self.spawn_file_create_task(&filename, &description, target_path);
                } else {
                    // Validation error
                    self.chat.messages.push(ChatMessage {
                        role: ChatRole::System,
                        content: result.message,
                    });
                }
            }
            TaskKind::FileEdit { path, instruction } => {
                match crate::file_ops::handle_edit_start(&path, &instruction, &self.root_directory)
                {
                    Ok((result, file_content)) => {
                        self.chat.messages.push(ChatMessage {
                            role: ChatRole::System,
                            content: result.message,
                        });
                        // Spawn LLM task for edit generation
                        let validated_path = self.root_directory.join(&path);
                        self.spawn_file_edit_task(
                            &path,
                            &instruction,
                            &file_content,
                            validated_path,
                        );
                    }
                    Err(result) => {
                        self.chat.messages.push(ChatMessage {
                            role: ChatRole::System,
                            content: result.message,
                        });
                    }
                }
            }
            TaskKind::FileFind { pattern } => {
                self.chat.messages.push(ChatMessage {
                    role: ChatRole::System,
                    content: "Searching...".to_string(),
                });
                let result = crate::file_ops::handle_find(
                    &pattern,
                    &self.root_directory,
                    &self.config.ignored_directories,
                );
                // Store find results for numeric selection
                if result.success {
                    self.last_find_results = result
                        .message
                        .lines()
                        .filter_map(|line| {
                            // Parse "N. path" format
                            let dot_pos = line.find(". ")?;
                            Some(PathBuf::from(line[dot_pos + 2..].trim()))
                        })
                        .collect();
                } else {
                    self.last_find_results.clear();
                }
                self.apply_file_op_result(result);
            }
            TaskKind::GeneralChat { input } => {
                self.handle_general_chat_task(&input);
            }
        }
    }

    /// Spawn an async LLM request with the given messages.
    /// Sends the result back via background_tx as LLMResponse or LLMError.
    fn spawn_llm_task(&self, messages: Vec<Message>) {
        let tx = self.background_tx.clone();
        let config = LLMConfig {
            base_url: self.config.base_url.clone(),
            api_key: self.config.api_key.clone(),
            model: self.config.model.clone(),
            temperature: self.config.temperature,
            max_tokens: self.config.max_tokens,
        };

        tokio::spawn(async move {
            let client = LLMClient::new(config);
            match client.chat(messages).await {
                Ok(response) => {
                    let _ = tx
                        .send(BackgroundMessage::LLMResponse {
                            content: response.content,
                            finish_reason: response.finish_reason,
                        })
                        .await;
                }
                Err(e) => {
                    let _ = tx.send(BackgroundMessage::LLMError(e.to_string())).await;
                }
            }
        });
    }

    /// Get the currently selected directory from the file tree.
    /// If a directory is selected, returns it. Otherwise returns root_directory.
    fn get_selected_directory(&self) -> PathBuf {
        if let Some(entry) = self.file_tree.entries.get(self.file_tree.selected_index) {
            if entry.is_dir && entry.path != PathBuf::from("..") {
                return self.root_directory.join(&entry.path);
            }
            // If a file is selected, use its parent directory
            if !entry.is_dir {
                if let Some(parent) = entry.path.parent() {
                    if !parent.as_os_str().is_empty() {
                        return self.root_directory.join(parent);
                    }
                }
            }
        }
        self.root_directory.clone()
    }

    /// Apply a FileOpResult to the app state.
    ///
    /// - Adds the result message to chat (Agent if success, System if failure)
    /// - If navigate_to is Some: expand all ancestor directories, reload file tree, select target
    /// - If open_file is Some: read the file and load it into the editor
    /// - If refresh_tree is true: reload the file tree
    fn apply_file_op_result(&mut self, result: crate::file_ops::FileOpResult) {
        // Log security rejections to agent output for audit
        if !result.success && result.message.contains("Security error") {
            self.agent_output.push(format!(
                "[SECURITY AUDIT] Rejected file operation: {}",
                result.message
            ));
        }

        // Add message to chat
        let role = if result.success {
            ChatRole::Agent
        } else {
            ChatRole::System
        };
        self.chat.messages.push(ChatMessage {
            role,
            content: result.message,
        });

        // Handle navigation: expand ancestors and select target
        if let Some(ref nav_path) = result.navigate_to {
            // Convert absolute path to relative for expanded_dirs
            if let Ok(relative) = nav_path.strip_prefix(&self.root_directory) {
                // Expand all ancestor directories
                let mut ancestor = PathBuf::new();
                for component in relative.components() {
                    ancestor = ancestor.join(component);
                    // Only add directories (not the final file entry)
                    let full = self.root_directory.join(&ancestor);
                    if full.is_dir() {
                        self.file_tree.expanded_dirs.insert(ancestor.clone());
                    }
                }
            }

            // Reload the file tree to reflect expanded state
            self.load_file_tree();

            // Find and select the target entry
            if let Ok(relative) = nav_path.strip_prefix(&self.root_directory) {
                let target = relative.to_path_buf();
                if let Some(idx) = self.file_tree.entries.iter().position(|e| e.path == target) {
                    self.file_tree.selected_index = idx;
                }
            }
        }

        // Handle opening a file in the editor
        if let Some(ref file_path) = result.open_file {
            if let Ok(content) = crate::tools::read_file_safe(file_path, self.config.max_file_kb) {
                let relative = file_path
                    .strip_prefix(&self.root_directory)
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|_| file_path.clone());
                self.editor.lines = content.lines().map(String::from).collect();
                self.editor.content = content;
                self.editor.file_path = Some(relative);
                self.editor.cursor_row = 0;
                self.editor.cursor_col = 0;
                self.editor.scroll_offset = 0;
            }
        }

        // Handle file tree refresh
        if result.refresh_tree {
            self.load_file_tree();
        }
    }

    /// Spawn an async LLM task for file creation.
    /// Sends the result back as BackgroundMessage::FileCreateResponse.
    fn spawn_file_create_task(&self, filename: &str, description: &str, target_path: PathBuf) {
        let tx = self.background_tx.clone();
        let config = LLMConfig {
            base_url: self.config.base_url.clone(),
            api_key: self.config.api_key.clone(),
            model: self.config.model.clone(),
            temperature: self.config.temperature,
            max_tokens: self.config.max_tokens,
        };

        // Build project context
        let project_context = format!(
            "Working directory: {}\nExisting files in current directory visible in tree.",
            self.root_directory.display()
        );

        let messages = crate::prompts::file_create_prompt(filename, description, &project_context);

        tokio::spawn(async move {
            let client = LLMClient::new(config);
            match client.chat(messages).await {
                Ok(response) => {
                    let _ = tx
                        .send(BackgroundMessage::FileCreateResponse {
                            target_path,
                            content: response.content,
                        })
                        .await;
                }
                Err(e) => {
                    let _ = tx.send(BackgroundMessage::LLMError(e.to_string())).await;
                }
            }
        });
    }

    /// Spawn an async LLM task for file editing.
    /// Sends the result back as BackgroundMessage::FileEditResponse.
    fn spawn_file_edit_task(
        &self,
        filename: &str,
        instruction: &str,
        file_content: &str,
        target_path: PathBuf,
    ) {
        let tx = self.background_tx.clone();
        let config = LLMConfig {
            base_url: self.config.base_url.clone(),
            api_key: self.config.api_key.clone(),
            model: self.config.model.clone(),
            temperature: self.config.temperature,
            max_tokens: self.config.max_tokens,
        };

        let messages = crate::prompts::file_edit_prompt(filename, file_content, instruction);
        let original_content = file_content.to_string();

        tokio::spawn(async move {
            let client = LLMClient::new(config);
            match client.chat(messages).await {
                Ok(response) => {
                    let _ = tx
                        .send(BackgroundMessage::FileEditResponse {
                            target_path,
                            original_content,
                            modified_content: response.content,
                        })
                        .await;
                }
                Err(e) => {
                    let _ = tx.send(BackgroundMessage::LLMError(e.to_string())).await;
                }
            }
        });
    }

    /// Handle the Review task: build prompt from editor content and send to LLM.
    fn handle_review_task(&mut self) {
        if self.editor.content.is_empty() {
            self.agent_output
                .push("No file open to review.".to_string());
            return;
        }

        // Check for secret file warning before sending content to LLM
        self.check_secret_file_warning();

        // Enforce 300 line limit
        let content = if self.editor.lines.len() > 300 {
            self.editor.lines[..300].join("\n")
        } else {
            self.editor.content.clone()
        };

        let filename = self.get_current_filename();
        let messages = crate::prompts::review_prompt(&filename, &content);
        self.chat.messages.push(ChatMessage {
            role: ChatRole::System,
            content: "Sending code for review...".to_string(),
        });
        self.spawn_llm_task(messages);
    }

    /// Handle the FixError task: extract snippet, build prompt, send to LLM.
    fn handle_fix_error_task(&mut self, line_number: u32, error_text: String) {
        if self.editor.content.is_empty() {
            self.agent_output.push("No file open to fix.".to_string());
            return;
        }

        // Check for secret file warning before sending content to LLM
        self.check_secret_file_warning();

        let snippet = self.extract_snippet(line_number);
        let filename = self.get_current_filename();
        let messages = crate::prompts::fix_error_prompt(&filename, &snippet, &error_text);
        self.chat.messages.push(ChatMessage {
            role: ChatRole::System,
            content: format!("Analyzing error around line {}...", line_number),
        });
        self.spawn_llm_task(messages);
    }

    /// Handle the Search task: call search_files and display results.
    fn handle_search_task(&mut self, term: &str) {
        let results = tools::search_files(
            &self.root_directory,
            term,
            &self.config.ignored_directories,
            100,
        );

        if results.is_empty() {
            self.agent_output
                .push(format!("No results found for: {term}"));
        } else {
            self.agent_output.push(format!(
                "Search results for \"{term}\" ({} matches):",
                results.len()
            ));
            for result in &results {
                self.agent_output.push(format!(
                    "  {}:{} — {}",
                    result.file_path.display(),
                    result.line_number,
                    result.line_content
                ));
            }
        }
    }

    /// Handle the TranslationCheck task: validate .html file, build prompt, send to LLM.
    fn handle_translation_check_task(&mut self) {
        if self.editor.content.is_empty() {
            self.agent_output
                .push("No file open for translation check.".to_string());
            return;
        }

        // Check for secret file warning before sending content to LLM
        self.check_secret_file_warning();

        let filename = self.get_current_filename();
        let messages = crate::prompts::translation_check_prompt(&filename, &self.editor.content);
        self.chat.messages.push(ChatMessage {
            role: ChatRole::System,
            content: "Checking for untranslated strings...".to_string(),
        });
        self.spawn_llm_task(messages);
    }

    /// Handle the HeaderDateTime task: build prompt, send to LLM.
    fn handle_header_datetime_task(&mut self) {
        if self.editor.content.is_empty() {
            self.agent_output
                .push("No file open for header modification.".to_string());
            return;
        }

        // Check for secret file warning before sending content to LLM
        self.check_secret_file_warning();

        let filename = self.get_current_filename();
        let messages = crate::prompts::header_datetime_prompt(&filename, &self.editor.content);
        self.chat.messages.push(ChatMessage {
            role: ChatRole::System,
            content: "Adding date/time to header...".to_string(),
        });
        self.spawn_llm_task(messages);
    }

    /// Handle the GeneralChat task: build prompt with or without file context.
    fn handle_general_chat_task(&mut self, input: &str) {
        // Query RAG for relevant context if enabled
        let rag_augmented_input = if let Some(ref rag_manager) = self.rag_manager {
            if rag_manager.is_enabled() {
                let top_k = rag_manager.top_k();
                let hits = rag_manager.query(input, top_k);
                crate::prompts::build_rag_augmented_prompt(input, &hits)
            } else {
                input.to_string()
            }
        } else {
            input.to_string()
        };

        let messages = if self.editor.content.is_empty() {
            crate::prompts::general_chat_prompt_no_file(&rag_augmented_input)
        } else {
            // Check for secret file warning before sending content to LLM
            self.check_secret_file_warning();

            let filename = self.get_current_filename();
            crate::prompts::general_chat_prompt(
                &filename,
                &self.editor.content,
                &rag_augmented_input,
            )
        };
        self.spawn_llm_task(messages);
    }

    fn submit_shell_command(&mut self, command: &str) {
        self.shell.last_command = Some(command.to_string());
        self.shell.is_running = true;
        self.shell.output_lines.push(format!("$ {command}"));

        // Check for dangerous commands and display warning
        if tools::is_dangerous_command(command) {
            self.shell
                .output_lines
                .push("⚠ Warning: This command may be dangerous!".to_string());
        }

        // Spawn the command execution as a background task
        let tx = self.background_tx.clone();
        let cmd = command.to_string();
        let cwd = self.root_directory.clone();
        tokio::spawn(async move {
            let (line_tx, mut line_rx) = mpsc::channel::<String>(100);

            // Forward lines from the command to the background channel
            let tx_clone = tx.clone();
            tokio::spawn(async move {
                while let Some(line) = line_rx.recv().await {
                    if tx_clone
                        .send(BackgroundMessage::CommandOutput { line })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            });

            let exit_code = tools::run_command(&cmd, &cwd, line_tx).await;
            let _ = tx
                .send(BackgroundMessage::CommandFinished { exit_code })
                .await;
        });
    }

    /// Check if the currently open file matches a secret pattern and display a warning.
    /// Should be called before any LLM operation that includes file content.
    fn check_secret_file_warning(&mut self) {
        if let Some(ref path) = self.editor.file_path {
            let full_path = self.root_directory.join(path);
            if tools::is_secret_file(&full_path, &self.config.secret_patterns) {
                self.agent_output.push(
                    "⚠ Warning: This file matches a secret pattern. Content will be sent to the LLM."
                        .to_string(),
                );
            }
        }
    }

    /// Process a message received from a background async task.
    pub fn handle_background_message(&mut self, msg: BackgroundMessage) {
        match msg {
            BackgroundMessage::LLMResponse {
                content,
                finish_reason: _,
            } => {
                // Check if response contains a diff
                if let Some(file_path) = self.editor.file_path.clone() {
                    let full_path = self.root_directory.join(&file_path);
                    if let Some(proposal) =
                        self.patch_system
                            .parse_llm_diff(&content, &full_path, &self.editor.content)
                    {
                        // Store the patch proposal
                        let reasoning = proposal.reasoning.clone();
                        let diff_text = proposal.diff_text.clone();
                        self.patch_system.store_proposal(proposal);

                        // Display reasoning in chat
                        if !reasoning.is_empty() {
                            self.chat.messages.push(ChatMessage {
                                role: ChatRole::Agent,
                                content: reasoning,
                            });
                        }

                        // Display diff in agent_output
                        self.agent_output.push("--- Proposed patch ---".to_string());
                        for line in diff_text.lines() {
                            self.agent_output.push(line.to_string());
                        }
                        self.agent_output
                            .push("Use F5 to accept, F6 to refuse, F7 to undo.".to_string());
                        return;
                    }
                }

                // No diff found — display as plain text in chat
                self.chat.messages.push(ChatMessage {
                    role: ChatRole::Agent,
                    content,
                });
            }
            BackgroundMessage::LLMError(error) => {
                self.chat.messages.push(ChatMessage {
                    role: ChatRole::System,
                    content: format!("Error: {error}"),
                });
            }
            BackgroundMessage::CommandOutput { line } => {
                self.shell.output_lines.push(line);
            }
            BackgroundMessage::CommandFinished { exit_code } => {
                self.shell.is_running = false;
                let indicator = if exit_code == 0 { "✓" } else { "✗" };
                self.shell
                    .output_lines
                    .push(format!("{indicator} Process exited with code {exit_code}"));
            }
            BackgroundMessage::RagIndexComplete { indexed_count } => {
                self.agent_output
                    .push(format!("RAG: indexed {indexed_count} entries"));
            }
            BackgroundMessage::RagIndexError(error) => {
                self.agent_output.push(format!("RAG error: {error}"));
            }
            BackgroundMessage::RagQueryResult { hits: _ } => {
                // Query results will be handled by the chat flow (task 10.6)
            }
            BackgroundMessage::FileCreateResponse {
                target_path,
                content,
            } => {
                let result = crate::file_ops::handle_create_complete(&target_path, &content);
                self.apply_file_op_result(result);
            }
            BackgroundMessage::FileEditResponse {
                target_path,
                original_content,
                modified_content,
            } => {
                // Generate a simple unified diff for display
                let diff_text =
                    generate_simple_diff(&target_path, &original_content, &modified_content);

                // Store as a patch proposal for F5/F6/F7 workflow
                let proposal = crate::patches::PatchProposal {
                    target_file: target_path.clone(),
                    diff_text: diff_text.clone(),
                    original_content,
                    proposed_content: modified_content,
                    reasoning: String::new(),
                };
                self.patch_system.store_proposal(proposal);

                // Display diff in chat
                self.chat.messages.push(ChatMessage {
                    role: ChatRole::Agent,
                    content: format!(
                        "Proposed changes to '{}':\n```diff\n{}\n```\nUse F5 to accept, F6 to refuse.",
                        target_path.display(),
                        diff_text
                    ),
                });

                // Also show in agent output
                self.agent_output.push("--- Proposed edit ---".to_string());
                for line in diff_text.lines() {
                    self.agent_output.push(line.to_string());
                }
                self.agent_output
                    .push("Use F5 to accept, F6 to refuse, F7 to undo.".to_string());
            }
        }
    }
}

/// Generate a simple unified diff between original and modified content.
///
/// Produces a basic line-by-line diff with `--- a/filename` and `+++ b/filename` headers,
/// a `@@ ... @@` hunk header, and lines prefixed with `-` (removed) or `+` (added).
/// Context lines (unchanged) are prefixed with a space.
fn generate_simple_diff(path: &Path, original: &str, modified: &str) -> String {
    let filename = path.display().to_string();
    let orig_lines: Vec<&str> = original.lines().collect();
    let mod_lines: Vec<&str> = modified.lines().collect();

    let mut diff = String::new();
    diff.push_str(&format!("--- a/{}\n", filename));
    diff.push_str(&format!("+++ b/{}\n", filename));

    // Simple diff: walk both line lists and emit removed/added/context lines
    // Use a basic longest-common-subsequence approach for small files,
    // or fall back to full remove-then-add for simplicity.
    let lcs = compute_lcs(&orig_lines, &mod_lines);

    // Build hunks from the LCS
    let mut orig_idx = 0;
    let mut mod_idx = 0;
    let mut hunk_lines: Vec<String> = Vec::new();
    let hunk_orig_start = 1;
    let hunk_mod_start = 1;
    let mut hunk_orig_count = 0;
    let mut hunk_mod_count = 0;

    for &(lcs_orig, lcs_mod) in &lcs {
        // Emit removed lines (in original but before this LCS match)
        while orig_idx < lcs_orig {
            hunk_lines.push(format!("-{}", orig_lines[orig_idx]));
            hunk_orig_count += 1;
            orig_idx += 1;
        }
        // Emit added lines (in modified but before this LCS match)
        while mod_idx < lcs_mod {
            hunk_lines.push(format!("+{}", mod_lines[mod_idx]));
            hunk_mod_count += 1;
            mod_idx += 1;
        }
        // Emit context line
        hunk_lines.push(format!(" {}", orig_lines[orig_idx]));
        hunk_orig_count += 1;
        hunk_mod_count += 1;
        orig_idx += 1;
        mod_idx += 1;
    }

    // Remaining lines after last LCS match
    while orig_idx < orig_lines.len() {
        hunk_lines.push(format!("-{}", orig_lines[orig_idx]));
        hunk_orig_count += 1;
        orig_idx += 1;
    }
    while mod_idx < mod_lines.len() {
        hunk_lines.push(format!("+{}", mod_lines[mod_idx]));
        hunk_mod_count += 1;
        mod_idx += 1;
    }

    if !hunk_lines.is_empty() {
        diff.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            hunk_orig_start, hunk_orig_count, hunk_mod_start, hunk_mod_count
        ));
        for line in &hunk_lines {
            diff.push_str(line);
            diff.push('\n');
        }
    }

    diff
}

/// Compute the longest common subsequence of two slices of lines.
/// Returns a vector of (orig_index, mod_index) pairs for matching lines.
fn compute_lcs<'a>(orig: &[&'a str], modified: &[&'a str]) -> Vec<(usize, usize)> {
    let n = orig.len();
    let m = modified.len();

    // Build LCS table
    let mut table = vec![vec![0u32; m + 1]; n + 1];
    for i in 1..=n {
        for j in 1..=m {
            if orig[i - 1] == modified[j - 1] {
                table[i][j] = table[i - 1][j - 1] + 1;
            } else {
                table[i][j] = table[i - 1][j].max(table[i][j - 1]);
            }
        }
    }

    // Backtrack to find the LCS
    let mut result = Vec::new();
    let mut i = n;
    let mut j = m;
    while i > 0 && j > 0 {
        if orig[i - 1] == modified[j - 1] {
            result.push((i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if table[i - 1][j] >= table[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    result.reverse();
    result
}

fn copy_path_recursive(source: &Path, destination: &Path) -> io::Result<()> {
    if source.is_dir() {
        fs::create_dir_all(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let child_source = entry.path();
            let child_destination = destination.join(entry.file_name());
            copy_path_recursive(&child_source, &child_destination)?;
        }
        Ok(())
    } else {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source, destination)?;
        Ok(())
    }
}

fn remove_path_recursive(path: &Path) -> io::Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::path::PathBuf;

    /// Helper to create a KeyEvent with no modifiers.
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// Helper to create a KeyEvent with Ctrl modifier.
    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    /// Helper to create a fresh App for testing.
    fn test_app() -> App {
        App::new(AppConfig::default(), PathBuf::from("/tmp/test"))
    }

    // --- Global key binding tests ---

    #[test]
    fn test_ctrl_q_quits() {
        let mut app = test_app();
        app.handle_key_event(ctrl(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn test_q_quits_when_not_in_chat_or_shell() {
        let mut app = test_app();
        app.focus = Pane::FileTree;
        app.handle_key_event(key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn test_q_does_not_quit_in_chat() {
        let mut app = test_app();
        app.focus = Pane::Chat;
        app.handle_key_event(key(KeyCode::Char('q')));
        assert!(!app.should_quit);
        // Instead it should insert 'q' into chat buffer
        assert_eq!(app.chat.input_buffer, "q");
    }

    #[test]
    fn test_q_does_not_quit_in_shell() {
        let mut app = test_app();
        app.focus = Pane::Shell;
        app.handle_key_event(key(KeyCode::Char('q')));
        assert!(!app.should_quit);
        assert_eq!(app.shell.input_buffer, "q");
    }

    #[test]
    fn test_tab_cycles_focus() {
        let mut app = test_app();
        assert_eq!(app.focus, Pane::FileTree);
        app.handle_key_event(key(KeyCode::Tab));
        assert_eq!(app.focus, Pane::Editor);
        app.handle_key_event(key(KeyCode::Tab));
        assert_eq!(app.focus, Pane::Chat);
        app.handle_key_event(key(KeyCode::Tab));
        assert_eq!(app.focus, Pane::Shell);
        app.handle_key_event(key(KeyCode::Tab));
        assert_eq!(app.focus, Pane::AgentOutput);
        app.handle_key_event(key(KeyCode::Tab));
        assert_eq!(app.focus, Pane::FileTree);
    }

    #[test]
    fn test_f1_focuses_shell() {
        let mut app = test_app();
        app.handle_key_event(key(KeyCode::F(1)));
        assert_eq!(app.focus, Pane::Shell);
    }

    #[test]
    fn test_f2_focuses_chat() {
        let mut app = test_app();
        app.handle_key_event(key(KeyCode::F(2)));
        assert_eq!(app.focus, Pane::Chat);
    }

    #[test]
    fn test_f3_focuses_editor() {
        let mut app = test_app();
        app.handle_key_event(key(KeyCode::F(3)));
        assert_eq!(app.focus, Pane::Editor);
    }

    #[test]
    fn test_f4_activates_create_file_mode() {
        let mut app = test_app();
        app.focus = Pane::Chat;
        app.handle_key_event(key(KeyCode::F(4)));
        assert!(app.create_file_mode);
        assert_eq!(app.create_file_buffer, "");
        assert_eq!(app.create_file_cursor, 0);
    }

    #[test]
    fn test_f11_activates_delete_confirmation() {
        let mut app = test_app();
        app.handle_key_event(key(KeyCode::F(11)));
        assert!(app.confirm_delete);
    }

    #[test]
    fn test_f9_toggles_about() {
        let mut app = test_app();
        assert!(!app.show_about);
        app.handle_key_event(key(KeyCode::F(9)));
        assert!(app.show_about);
    }

    #[test]
    fn test_about_dismissed_by_any_key() {
        let mut app = test_app();
        app.show_about = true;
        app.handle_key_event(key(KeyCode::Esc));
        assert!(!app.show_about);
    }

    #[test]
    fn test_about_dismissed_by_char_key() {
        let mut app = test_app();
        app.show_about = true;
        app.handle_key_event(key(KeyCode::Char('a')));
        assert!(!app.show_about);
    }

    #[test]
    fn test_ctrl_s_adds_save_message() {
        let mut app = test_app();
        app.handle_key_event(ctrl(KeyCode::Char('s')));
        assert_eq!(app.agent_output.len(), 1);
        assert!(app.agent_output[0].contains("No file open to save"));
    }

    #[test]
    fn test_ctrl_o_toggles_commander_mode() {
        let mut app = test_app();
        app.handle_key_event(ctrl(KeyCode::Char('o')));
        assert!(app.commander_mode);
        app.handle_key_event(ctrl(KeyCode::Char('o')));
        assert!(!app.commander_mode);
    }

    #[test]
    fn test_ctrl_m_alias_toggles_commander_mode() {
        let mut app = test_app();
        app.handle_key_event(ctrl(KeyCode::Char('m')));
        assert!(app.commander_mode);
    }

    #[tokio::test]
    async fn test_ctrl_r_reruns_last_command() {
        let mut app = test_app();
        app.shell.last_command = Some("echo hello".to_string());
        app.handle_key_event(ctrl(KeyCode::Char('r')));
        assert!(app
            .shell
            .output_lines
            .iter()
            .any(|l| l.contains("echo hello")));
    }

    #[tokio::test]
    async fn test_ctrl_d_submits_git_diff_command() {
        let mut app = test_app();
        app.handle_key_event(ctrl(KeyCode::Char('d')));
        // Should submit "git diff" as a shell command
        assert_eq!(app.shell.last_command, Some("git diff".to_string()));
        assert!(app.shell.is_running);
        assert!(app
            .shell
            .output_lines
            .iter()
            .any(|l| l.contains("$ git diff")));
    }

    #[test]
    fn test_f5_accept_patch_no_pending() {
        let mut app = test_app();
        app.handle_key_event(key(KeyCode::F(5)));
        assert_eq!(app.agent_output.len(), 1);
        assert!(app.agent_output[0].contains("No pending patch"));
    }

    #[test]
    fn test_f6_refuse_patch_no_pending() {
        let mut app = test_app();
        app.handle_key_event(key(KeyCode::F(6)));
        assert_eq!(app.agent_output.len(), 1);
        assert!(app.agent_output[0].contains("No pending patch"));
    }

    #[test]
    fn test_f7_undo_patch_no_applied() {
        let mut app = test_app();
        app.handle_key_event(key(KeyCode::F(7)));
        assert_eq!(app.agent_output.len(), 1);
        assert!(app.agent_output[0].contains("No patch to undo"));
    }

    // --- Chat input tests ---

    #[test]
    fn test_chat_input_char_insertion() {
        let mut app = test_app();
        app.focus = Pane::Chat;
        app.handle_key_event(key(KeyCode::Char('h')));
        app.handle_key_event(key(KeyCode::Char('i')));
        assert_eq!(app.chat.input_buffer, "hi");
        assert_eq!(app.chat.cursor_pos, 2);
    }

    #[test]
    fn test_chat_input_backspace() {
        let mut app = test_app();
        app.focus = Pane::Chat;
        app.chat.input_buffer = "hello".to_string();
        app.chat.cursor_pos = 5;
        app.handle_key_event(key(KeyCode::Backspace));
        assert_eq!(app.chat.input_buffer, "hell");
        assert_eq!(app.chat.cursor_pos, 4);
    }

    #[test]
    fn test_chat_input_backspace_at_start() {
        let mut app = test_app();
        app.focus = Pane::Chat;
        app.chat.input_buffer = "hello".to_string();
        app.chat.cursor_pos = 0;
        app.handle_key_event(key(KeyCode::Backspace));
        assert_eq!(app.chat.input_buffer, "hello");
        assert_eq!(app.chat.cursor_pos, 0);
    }

    #[test]
    fn test_chat_input_enter_submits() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut app = test_app();
            app.focus = Pane::Chat;
            app.chat.input_buffer = "hello agent".to_string();
            app.chat.cursor_pos = 11;
            app.handle_key_event(key(KeyCode::Enter));
            assert!(app.chat.input_buffer.is_empty());
            assert_eq!(app.chat.cursor_pos, 0);
            assert_eq!(app.chat.messages.len(), 1);
            assert_eq!(app.chat.messages[0].content, "hello agent");
            assert_eq!(app.chat.messages[0].role, ChatRole::User);
        });
    }

    #[test]
    fn test_chat_input_enter_empty_does_nothing() {
        let mut app = test_app();
        app.focus = Pane::Chat;
        app.chat.input_buffer = "   ".to_string();
        app.chat.cursor_pos = 3;
        app.handle_key_event(key(KeyCode::Enter));
        // Empty/whitespace input should not submit
        assert!(app.chat.messages.is_empty());
    }

    #[test]
    fn test_chat_text_command_accept() {
        let mut app = test_app();
        app.focus = Pane::Chat;
        app.chat.input_buffer = "accept".to_string();
        app.chat.cursor_pos = 6;
        app.handle_key_event(key(KeyCode::Enter));
        // Should have added user message and triggered accept patch
        assert_eq!(app.chat.messages.len(), 1);
        assert!(app
            .agent_output
            .iter()
            .any(|m| m.contains("No pending patch")));
    }

    #[test]
    fn test_chat_text_command_refuse() {
        let mut app = test_app();
        app.focus = Pane::Chat;
        app.chat.input_buffer = "reject".to_string();
        app.chat.cursor_pos = 6;
        app.handle_key_event(key(KeyCode::Enter));
        assert!(app
            .agent_output
            .iter()
            .any(|m| m.contains("No pending patch")));
    }

    #[test]
    fn test_chat_text_command_undo() {
        let mut app = test_app();
        app.focus = Pane::Chat;
        app.chat.input_buffer = "undo".to_string();
        app.chat.cursor_pos = 4;
        app.handle_key_event(key(KeyCode::Enter));
        assert!(app
            .agent_output
            .iter()
            .any(|m| m.contains("No patch to undo")));
    }

    #[test]
    fn test_chat_cursor_left_right() {
        let mut app = test_app();
        app.focus = Pane::Chat;
        app.chat.input_buffer = "abc".to_string();
        app.chat.cursor_pos = 3;
        app.handle_key_event(key(KeyCode::Left));
        assert_eq!(app.chat.cursor_pos, 2);
        app.handle_key_event(key(KeyCode::Left));
        assert_eq!(app.chat.cursor_pos, 1);
        app.handle_key_event(key(KeyCode::Right));
        assert_eq!(app.chat.cursor_pos, 2);
    }

    // --- Shell input tests ---

    #[test]
    fn test_shell_input_char_insertion() {
        let mut app = test_app();
        app.focus = Pane::Shell;
        app.handle_key_event(key(KeyCode::Char('l')));
        app.handle_key_event(key(KeyCode::Char('s')));
        assert_eq!(app.shell.input_buffer, "ls");
        assert_eq!(app.shell.cursor_pos, 2);
    }

    #[tokio::test]
    async fn test_shell_input_enter_submits() {
        let mut app = test_app();
        app.focus = Pane::Shell;
        app.shell.input_buffer = "echo test".to_string();
        app.shell.cursor_pos = 9;
        app.handle_key_event(key(KeyCode::Enter));
        assert!(app.shell.input_buffer.is_empty());
        assert_eq!(app.shell.cursor_pos, 0);
        assert_eq!(app.shell.last_command, Some("echo test".to_string()));
        assert!(app.shell.is_running);
        assert!(app
            .shell
            .output_lines
            .iter()
            .any(|l| l.contains("$ echo test")));
    }

    #[test]
    fn test_shell_input_backspace() {
        let mut app = test_app();
        app.focus = Pane::Shell;
        app.shell.input_buffer = "ls -la".to_string();
        app.shell.cursor_pos = 6;
        app.handle_key_event(key(KeyCode::Backspace));
        assert_eq!(app.shell.input_buffer, "ls -l");
        assert_eq!(app.shell.cursor_pos, 5);
    }

    // --- File tree input tests ---

    #[test]
    fn test_file_tree_down_navigation() {
        let mut app = test_app();
        app.focus = Pane::FileTree;
        app.file_tree.entries = vec![
            FileEntry {
                path: PathBuf::from("a"),
                name: "a".to_string(),
                is_dir: false,
                depth: 0,
            },
            FileEntry {
                path: PathBuf::from("b"),
                name: "b".to_string(),
                is_dir: false,
                depth: 0,
            },
            FileEntry {
                path: PathBuf::from("c"),
                name: "c".to_string(),
                is_dir: false,
                depth: 0,
            },
        ];
        assert_eq!(app.file_tree.selected_index, 0);
        app.handle_key_event(key(KeyCode::Down));
        assert_eq!(app.file_tree.selected_index, 1);
        app.handle_key_event(key(KeyCode::Down));
        assert_eq!(app.file_tree.selected_index, 2);
        // Should not go past the end
        app.handle_key_event(key(KeyCode::Down));
        assert_eq!(app.file_tree.selected_index, 2);
    }

    #[test]
    fn test_file_tree_up_navigation() {
        let mut app = test_app();
        app.focus = Pane::FileTree;
        app.file_tree.entries = vec![
            FileEntry {
                path: PathBuf::from("a"),
                name: "a".to_string(),
                is_dir: false,
                depth: 0,
            },
            FileEntry {
                path: PathBuf::from("b"),
                name: "b".to_string(),
                is_dir: false,
                depth: 0,
            },
        ];
        app.file_tree.selected_index = 1;
        app.handle_key_event(key(KeyCode::Up));
        assert_eq!(app.file_tree.selected_index, 0);
        // Should not go below 0
        app.handle_key_event(key(KeyCode::Up));
        assert_eq!(app.file_tree.selected_index, 0);
    }

    // --- Editor input tests ---

    #[test]
    fn test_editor_arrow_keys() {
        let mut app = test_app();
        app.focus = Pane::Editor;
        app.editor.lines = vec![
            "line1".to_string(),
            "line2".to_string(),
            "line3".to_string(),
        ];
        app.editor.cursor_row = 1;
        app.editor.cursor_col = 2;

        app.handle_key_event(key(KeyCode::Up));
        assert_eq!(app.editor.cursor_row, 0);

        app.handle_key_event(key(KeyCode::Down));
        assert_eq!(app.editor.cursor_row, 1);

        app.handle_key_event(key(KeyCode::Left));
        assert_eq!(app.editor.cursor_col, 1);

        app.handle_key_event(key(KeyCode::Right));
        assert_eq!(app.editor.cursor_col, 2);
    }

    #[test]
    fn test_editor_up_at_top_stays() {
        let mut app = test_app();
        app.focus = Pane::Editor;
        app.editor.lines = vec!["line1".to_string()];
        app.editor.cursor_row = 0;
        app.handle_key_event(key(KeyCode::Up));
        assert_eq!(app.editor.cursor_row, 0);
    }

    #[test]
    fn test_pane_next_cycles_through_all_panes() {
        assert_eq!(Pane::FileTree.next(), Pane::Editor);
        assert_eq!(Pane::Editor.next(), Pane::Chat);
        assert_eq!(Pane::Chat.next(), Pane::Shell);
        assert_eq!(Pane::Shell.next(), Pane::AgentOutput);
        assert_eq!(Pane::AgentOutput.next(), Pane::FileTree);
    }

    #[test]
    fn test_pane_full_cycle_returns_to_start() {
        let start = Pane::FileTree;
        let result = start.next().next().next().next().next();
        assert_eq!(result, start);
    }

    #[test]
    fn test_app_new_initializes_with_defaults() {
        let config = AppConfig::default();
        let root = PathBuf::from("/tmp/test-project");
        let app = App::new(config, root.clone());

        assert_eq!(app.root_directory, root);
        assert_eq!(app.focus, Pane::FileTree);
        assert!(!app.show_about);
        assert!(!app.should_quit);
        assert!(app.file_tree.entries.is_empty());
        assert_eq!(app.file_tree.selected_index, 0);
        assert_eq!(app.editor.cursor_row, 0);
        assert_eq!(app.editor.cursor_col, 0);
        assert!(app.editor.file_path.is_none());
        assert!(app.chat.messages.is_empty());
        assert_eq!(app.chat.cursor_pos, 0);
        assert!(app.shell.output_lines.is_empty());
        assert!(!app.shell.is_running);
        assert!(app.shell.last_command.is_none());
        assert!(app.agent_output.is_empty());
    }

    #[test]
    fn test_file_tree_state_default() {
        let state = FileTreeState::default();
        assert!(state.entries.is_empty());
        assert_eq!(state.selected_index, 0);
        assert_eq!(state.scroll_offset, 0);
        assert!(state.expanded_dirs.is_empty());
    }

    #[test]
    fn test_editor_state_default() {
        let state = EditorState::default();
        assert!(state.content.is_empty());
        assert!(state.lines.is_empty());
        assert_eq!(state.cursor_row, 0);
        assert_eq!(state.cursor_col, 0);
        assert_eq!(state.scroll_offset, 0);
        assert!(state.file_path.is_none());
    }

    #[test]
    fn test_chat_state_default() {
        let state = ChatState::default();
        assert!(state.input_buffer.is_empty());
        assert_eq!(state.cursor_pos, 0);
        assert!(state.messages.is_empty());
        assert_eq!(state.scroll_offset, 0);
    }

    #[test]
    fn test_shell_state_default() {
        let state = ShellState::default();
        assert!(state.input_buffer.is_empty());
        assert_eq!(state.cursor_pos, 0);
        assert!(state.output_lines.is_empty());
        assert_eq!(state.scroll_offset, 0);
        assert!(state.last_command.is_none());
        assert!(!state.is_running);
    }

    #[test]
    fn test_commander_f5_copies_file_to_other_pane() {
        let tmp = tempfile::tempdir().unwrap();
        let left = tmp.path().join("left");
        let right = tmp.path().join("right");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::create_dir_all(&right).unwrap();
        std::fs::write(left.join("note.txt"), "hello").unwrap();

        let mut app = App::new(AppConfig::default(), tmp.path().to_path_buf());
        app.commander_mode = true;
        app.commander = CommanderState::new(left.clone());
        app.commander.right.current_dir = right.clone();
        app.load_commander_entries(CommanderPane::Left);
        app.load_commander_entries(CommanderPane::Right);
        app.commander.active_pane = CommanderPane::Left;
        app.commander.left.selected_index = 1;

        app.handle_key_event(key(KeyCode::F(5)));

        assert_eq!(
            std::fs::read_to_string(right.join("note.txt")).unwrap(),
            "hello"
        );
    }

    #[test]
    fn test_commander_f4_opens_file_in_commander_editor() {
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("edit.txt");
        std::fs::write(&file_path, "new").unwrap();

        let mut app = App::new(AppConfig::default(), tmp.path().to_path_buf());
        app.commander_mode = true;
        app.commander = CommanderState::new(tmp.path().to_path_buf());
        app.load_commander_entries(CommanderPane::Left);
        app.commander.left.selected_index = 1;

        app.handle_key_event(key(KeyCode::F(4)));

        assert!(app.commander_mode);
        assert!(app.commander.editor.is_some());
        assert_eq!(
            app.commander
                .editor
                .as_ref()
                .map(|editor| editor.file_path.clone()),
            Some(file_path)
        );
        assert!(app.editor.file_path.is_none());
    }

    #[test]
    fn test_commander_ctrl_q_returns_to_main_view_without_changing_editor() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = App::new(AppConfig::default(), tmp.path().to_path_buf());
        app.editor.file_path = Some(PathBuf::from("keep.rs"));
        app.editor.content = "keep".to_string();
        app.focus = Pane::Chat;
        app.commander_mode = true;

        app.handle_key_event(ctrl(KeyCode::Char('q')));

        assert!(!app.commander_mode);
        assert_eq!(app.focus, Pane::Chat);
        assert_eq!(app.editor.file_path, Some(PathBuf::from("keep.rs")));
        assert_eq!(app.editor.content, "keep");
    }

    // --- File tree loading tests ---

    #[test]
    fn test_load_file_tree_basic() {
        let tmp = tempfile::tempdir().unwrap();
        // Create some files and directories
        std::fs::create_dir(tmp.path().join("src")).unwrap();
        std::fs::create_dir(tmp.path().join("docs")).unwrap();
        std::fs::write(tmp.path().join("README.md"), "hello").unwrap();
        std::fs::write(tmp.path().join("Cargo.toml"), "toml").unwrap();
        std::fs::write(tmp.path().join("src/main.rs"), "fn main() {}").unwrap();

        let mut app = App::new(AppConfig::default(), tmp.path().to_path_buf());
        app.load_file_tree();

        // Should have ".." entry + dirs first (docs, src), then files (Cargo.toml, README.md)
        assert_eq!(app.file_tree.entries.len(), 5);
        assert!(app.file_tree.entries[0].is_dir);
        assert_eq!(app.file_tree.entries[0].name, "..");
        assert!(app.file_tree.entries[1].is_dir);
        assert_eq!(app.file_tree.entries[1].name, "docs");
        assert!(app.file_tree.entries[2].is_dir);
        assert_eq!(app.file_tree.entries[2].name, "src");
        assert!(!app.file_tree.entries[3].is_dir);
        assert_eq!(app.file_tree.entries[3].name, "Cargo.toml");
        assert!(!app.file_tree.entries[4].is_dir);
        assert_eq!(app.file_tree.entries[4].name, "README.md");
    }

    #[test]
    fn test_load_file_tree_filters_ignored_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join(".git")).unwrap();
        std::fs::create_dir(tmp.path().join("node_modules")).unwrap();
        std::fs::create_dir(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("main.rs"), "fn main() {}").unwrap();

        let mut app = App::new(AppConfig::default(), tmp.path().to_path_buf());
        app.load_file_tree();

        // .git and node_modules should be filtered out
        let names: Vec<&str> = app
            .file_tree
            .entries
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert!(!names.contains(&".git"));
        assert!(!names.contains(&"node_modules"));
        assert!(names.contains(&"src"));
        assert!(names.contains(&"main.rs"));
    }

    #[test]
    fn test_load_file_tree_expanded_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/lib.rs"), "// lib").unwrap();
        std::fs::write(tmp.path().join("main.rs"), "fn main() {}").unwrap();

        let mut app = App::new(AppConfig::default(), tmp.path().to_path_buf());

        // Without expanding, only top-level entries (+ ".." entry)
        app.load_file_tree();
        assert_eq!(app.file_tree.entries.len(), 3); // ".." + src dir + main.rs

        // Expand src
        app.file_tree.expanded_dirs.insert(PathBuf::from("src"));
        app.load_file_tree();
        assert_eq!(app.file_tree.entries.len(), 4); // ".." + src dir + src/lib.rs + main.rs

        // Verify the child entry (index 2 because ".." is 0, "src" is 1)
        assert_eq!(app.file_tree.entries[2].name, "lib.rs");
        assert_eq!(app.file_tree.entries[2].depth, 1);
        assert_eq!(app.file_tree.entries[2].path, PathBuf::from("src/lib.rs"));
    }

    #[test]
    fn test_handle_file_tree_select_directory_toggle() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/lib.rs"), "// lib").unwrap();

        let mut app = App::new(AppConfig::default(), tmp.path().to_path_buf());
        app.load_file_tree();

        // Select the directory (index 1, since ".." is at index 0)
        app.file_tree.selected_index = 1;
        assert!(app.file_tree.entries[1].is_dir);
        assert_eq!(app.file_tree.entries[1].name, "src");

        // Selecting a directory enters it (changes root)
        app.handle_file_tree_select();
        // Root should now be the src directory
        assert!(app.root_directory.ends_with("src"));
        // expanded_dirs should be cleared on directory entry
        assert!(app.file_tree.expanded_dirs.is_empty());
    }

    #[test]
    fn test_handle_file_tree_select_file_opens_in_editor() {
        let tmp = tempfile::tempdir().unwrap();
        let content = "fn main() {\n    println!(\"hello\");\n}\n";
        std::fs::write(tmp.path().join("main.rs"), content).unwrap();

        let mut app = App::new(AppConfig::default(), tmp.path().to_path_buf());
        app.load_file_tree();

        // Select the file (index 1, since ".." is at index 0)
        app.file_tree.selected_index = 1;
        assert!(!app.file_tree.entries[1].is_dir);

        app.handle_file_tree_select();

        // Editor should be updated
        assert_eq!(app.editor.content, content);
        assert_eq!(app.editor.lines.len(), 3);
        assert_eq!(app.editor.file_path, Some(PathBuf::from("main.rs")));
        assert_eq!(app.editor.cursor_row, 0);
        assert_eq!(app.editor.cursor_col, 0);
        assert_eq!(app.editor.scroll_offset, 0);
    }

    #[test]
    fn test_handle_file_tree_select_binary_file_shows_error() {
        let tmp = tempfile::tempdir().unwrap();
        // Write a binary file (contains null bytes)
        std::fs::write(tmp.path().join("image.bin"), b"\x00\x01\x02\x03").unwrap();

        let mut app = App::new(AppConfig::default(), tmp.path().to_path_buf());
        app.load_file_tree();

        // Select the binary file (index 1, since ".." is at index 0)
        app.file_tree.selected_index = 1;
        app.handle_file_tree_select();

        // Should have error in agent_output
        assert_eq!(app.agent_output.len(), 1);
        assert!(app.agent_output[0].contains("Binary file"));
        // Editor should not be updated
        assert!(app.editor.content.is_empty());
    }

    #[test]
    fn test_handle_refresh_tree_reloads() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.txt"), "hello").unwrap();

        let mut app = App::new(AppConfig::default(), tmp.path().to_path_buf());
        app.load_file_tree();
        assert_eq!(app.file_tree.entries.len(), 2); // ".." + a.txt

        // Add a new file
        std::fs::write(tmp.path().join("b.txt"), "world").unwrap();

        // Refresh should pick up the new file
        app.handle_refresh_tree();
        assert_eq!(app.file_tree.entries.len(), 3); // ".." + a.txt + b.txt
    }

    #[test]
    fn test_settings_switch_to_remote_provider_clears_local_api_key() {
        let mut app = test_app();
        app.settings.selected_provider = app
            .settings
            .providers
            .iter()
            .position(|(_, key)| key == "deepseek")
            .unwrap();

        app.settings.apply_provider_preset();

        assert_eq!(app.settings.base_url_buffer, "https://api.deepseek.com/v1");
        assert_eq!(app.settings.model_buffer, "deepseek-chat");
        assert_eq!(app.settings.api_key_buffer, "");
    }

    #[test]
    fn test_settings_switch_to_local_provider_restores_local_api_key() {
        let config = AppConfig {
            provider: "openai".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            model: "gpt-4o-mini".to_string(),
            ..AppConfig::default()
        };
        let mut app = App::new(config, PathBuf::from("/tmp/test"));
        app.settings.selected_provider = app
            .settings
            .providers
            .iter()
            .position(|(_, key)| key == "ollama")
            .unwrap();

        app.settings.apply_provider_preset();

        assert_eq!(app.settings.base_url_buffer, "http://127.0.0.1:11434/v1");
        assert_eq!(app.settings.model_buffer, "qwen2.5-coder:7b");
        assert_eq!(app.settings.api_key_buffer, "local");
    }

    #[test]
    fn test_load_file_tree_clamps_selected_index() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.txt"), "a").unwrap();
        std::fs::write(tmp.path().join("b.txt"), "b").unwrap();
        std::fs::write(tmp.path().join("c.txt"), "c").unwrap();

        let mut app = App::new(AppConfig::default(), tmp.path().to_path_buf());
        app.load_file_tree();
        app.file_tree.selected_index = 3; // last item (index 3 = c.txt, after "..", a.txt, b.txt)

        // Remove files so tree shrinks
        std::fs::remove_file(tmp.path().join("b.txt")).unwrap();
        std::fs::remove_file(tmp.path().join("c.txt")).unwrap();
        app.load_file_tree();

        // selected_index should be clamped to last valid index (".." + a.txt = 2 entries, max index = 1)
        assert_eq!(app.file_tree.selected_index, 1);
    }

    // --- Background message handling tests ---

    #[test]
    fn test_handle_background_message_llm_response_plain_text() {
        let mut app = test_app();
        // No file open, so no diff parsing — should display as plain text
        let msg = BackgroundMessage::LLMResponse {
            content: "Here is my answer.".to_string(),
            finish_reason: "stop".to_string(),
        };
        app.handle_background_message(msg);
        assert_eq!(app.chat.messages.len(), 1);
        assert_eq!(app.chat.messages[0].role, ChatRole::Agent);
        assert_eq!(app.chat.messages[0].content, "Here is my answer.");
    }

    #[test]
    fn test_handle_background_message_llm_response_no_diff_with_file_open() {
        let mut app = test_app();
        // Open a file but send a response without a diff
        app.editor.file_path = Some(PathBuf::from("test.rs"));
        app.editor.content = "fn main() {}".to_string();
        let msg = BackgroundMessage::LLMResponse {
            content: "This code looks good!".to_string(),
            finish_reason: "stop".to_string(),
        };
        app.handle_background_message(msg);
        // Should display as plain text since no diff was found
        assert_eq!(app.chat.messages.len(), 1);
        assert_eq!(app.chat.messages[0].role, ChatRole::Agent);
        assert_eq!(app.chat.messages[0].content, "This code looks good!");
    }

    #[test]
    fn test_handle_background_message_llm_response_with_diff() {
        let mut app = test_app();
        app.editor.file_path = Some(PathBuf::from("test.rs"));
        app.editor.content = "fn main() {\n    println!(\"hello\");\n}\n".to_string();

        let response_with_diff = "Here is the fix:\n```diff\n--- a/test.rs\n+++ b/test.rs\n@@ -1,3 +1,3 @@\n fn main() {\n-    println!(\"hello\");\n+    println!(\"world\");\n }\n```\n";
        let msg = BackgroundMessage::LLMResponse {
            content: response_with_diff.to_string(),
            finish_reason: "stop".to_string(),
        };
        app.handle_background_message(msg);

        // Should have stored a patch proposal
        assert!(app.patch_system.has_pending());

        // Should have displayed diff in agent_output
        assert!(app
            .agent_output
            .iter()
            .any(|l| l.contains("Proposed patch")));
        assert!(app.agent_output.iter().any(|l| l.contains("F5 to accept")));
    }

    #[test]
    fn test_handle_background_message_llm_response_with_diff_and_reasoning() {
        let mut app = test_app();
        app.editor.file_path = Some(PathBuf::from("test.rs"));
        app.editor.content = "fn main() {\n    println!(\"hello\");\n}\n".to_string();

        let response_with_diff = "The issue is the greeting message.\n```diff\n--- a/test.rs\n+++ b/test.rs\n@@ -1,3 +1,3 @@\n fn main() {\n-    println!(\"hello\");\n+    println!(\"world\");\n }\n```\n";
        let msg = BackgroundMessage::LLMResponse {
            content: response_with_diff.to_string(),
            finish_reason: "stop".to_string(),
        };
        app.handle_background_message(msg);

        // Reasoning should be displayed in chat
        assert!(app
            .chat
            .messages
            .iter()
            .any(|m| { m.role == ChatRole::Agent && m.content.contains("greeting message") }));
    }

    #[test]
    fn test_handle_background_message_llm_error() {
        let mut app = test_app();
        let msg = BackgroundMessage::LLMError("Connection refused".to_string());
        app.handle_background_message(msg);
        assert_eq!(app.chat.messages.len(), 1);
        assert_eq!(app.chat.messages[0].role, ChatRole::System);
        assert_eq!(app.chat.messages[0].content, "Error: Connection refused");
    }

    #[test]
    fn test_handle_background_message_command_output() {
        let mut app = test_app();
        let msg = BackgroundMessage::CommandOutput {
            line: "Hello from command".to_string(),
        };
        app.handle_background_message(msg);
        assert_eq!(app.shell.output_lines.len(), 1);
        assert_eq!(app.shell.output_lines[0], "Hello from command");
    }

    #[test]
    fn test_handle_background_message_command_output_multiple_lines() {
        let mut app = test_app();
        app.handle_background_message(BackgroundMessage::CommandOutput {
            line: "line 1".to_string(),
        });
        app.handle_background_message(BackgroundMessage::CommandOutput {
            line: "line 2".to_string(),
        });
        app.handle_background_message(BackgroundMessage::CommandOutput {
            line: "line 3".to_string(),
        });
        assert_eq!(app.shell.output_lines.len(), 3);
        assert_eq!(app.shell.output_lines[0], "line 1");
        assert_eq!(app.shell.output_lines[1], "line 2");
        assert_eq!(app.shell.output_lines[2], "line 3");
    }

    #[test]
    fn test_handle_background_message_command_finished_success() {
        let mut app = test_app();
        app.shell.is_running = true;
        let msg = BackgroundMessage::CommandFinished { exit_code: 0 };
        app.handle_background_message(msg);
        assert!(!app.shell.is_running);
        assert_eq!(app.shell.output_lines.len(), 1);
        assert!(app.shell.output_lines[0].contains("✓"));
        assert!(app.shell.output_lines[0].contains("code 0"));
    }

    #[test]
    fn test_handle_background_message_command_finished_failure() {
        let mut app = test_app();
        app.shell.is_running = true;
        let msg = BackgroundMessage::CommandFinished { exit_code: 1 };
        app.handle_background_message(msg);
        assert!(!app.shell.is_running);
        assert_eq!(app.shell.output_lines.len(), 1);
        assert!(app.shell.output_lines[0].contains("✗"));
        assert!(app.shell.output_lines[0].contains("code 1"));
    }

    #[test]
    fn test_handle_background_message_command_finished_nonzero() {
        let mut app = test_app();
        app.shell.is_running = true;
        let msg = BackgroundMessage::CommandFinished { exit_code: 127 };
        app.handle_background_message(msg);
        assert!(!app.shell.is_running);
        assert!(app.shell.output_lines[0].contains("✗"));
        assert!(app.shell.output_lines[0].contains("code 127"));
    }

    // =========================================================================
    // Bug Condition Exploration Property Tests
    // Validates: Requirements 1.1, 1.3, 1.4
    //
    // These tests are EXPECTED TO FAIL on unfixed code. Failure confirms the
    // bugs exist. DO NOT fix the code to make these pass — that is task 3.
    // =========================================================================

    mod bug_condition_exploration {
        use super::*;
        use proptest::prelude::*;

        /// Helper: create an App with a file tree containing `entries_len` entries.
        fn app_with_file_tree(entries_len: usize) -> App {
            let mut app = test_app();
            app.focus = Pane::FileTree;
            app.file_tree.entries = (0..entries_len)
                .map(|i| FileEntry {
                    path: PathBuf::from(format!("file_{}.txt", i)),
                    name: format!("file_{}.txt", i),
                    is_dir: false,
                    depth: 0,
                })
                .collect();
            app.file_tree.selected_index = 0;
            app.file_tree.scroll_offset = 0;
            app
        }

        /// Helper: create a KeyEvent with Alt modifier.
        fn alt_key(code: KeyCode) -> KeyEvent {
            KeyEvent::new(code, KeyModifiers::ALT)
        }

        // **Validates: Requirements 1.1**
        //
        // Property 1: Bug Condition - File Tree Scroll-Follow
        //
        // For any file tree with entries_len entries and a visible_height viewport,
        // after navigating Down nav_steps times, the scroll_offset MUST adjust so
        // that selected_index is within [scroll_offset, scroll_offset + visible_height).
        //
        // On UNFIXED code, scroll_offset stays 0 regardless of navigation, so this
        // will fail whenever selected_index >= visible_height.
        proptest! {
            #[test]
            fn bug_condition_file_tree_scroll_follow(
                entries_len in 20..100usize,
                visible_height in 5..20usize,
                nav_steps in 1..50usize,
            ) {
                let mut app = app_with_file_tree(entries_len);
                app.file_tree.visible_height = visible_height;

                // Navigate Down nav_steps times
                let effective_steps = nav_steps.min(entries_len - 1);
                for _ in 0..effective_steps {
                    app.handle_key_event(key(KeyCode::Down));
                }

                // After navigation, selected_index should be within the visible window
                // defined by [scroll_offset, scroll_offset + visible_height)
                let selected = app.file_tree.selected_index;
                let scroll = app.file_tree.scroll_offset;

                // The invariant: selection must be visible
                prop_assert!(
                    selected >= scroll && selected < scroll + visible_height,
                    "Bug: selected_index={} is outside visible window [scroll_offset={}, scroll_offset+visible_height={}). \
                     scroll_offset was never adjusted after navigating {} steps in a tree with {} entries and viewport height {}.",
                    selected, scroll, scroll + visible_height, effective_steps, entries_len, visible_height
                );
            }
        }

        // **Validates: Requirements 1.3, 1.4**
        //
        // Property 2: Bug Condition - Chat History Recall
        //
        // After submitting history_entries messages and pressing Alt+Up, the
        // input_buffer MUST contain the last submitted message.
        //
        // On UNFIXED code, there is no history mechanism, so input_buffer will
        // remain empty after Alt+Up.
        proptest! {
            #[test]
            fn bug_condition_chat_history_recall(
                history_entries in 1..10usize,
            ) {
                // submit_chat_input dispatches tasks that require a Tokio runtime
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    let mut app = test_app();
                    app.focus = Pane::Chat;

                    // Submit history_entries messages
                    let mut last_submitted = String::new();
                    for i in 0..history_entries {
                        let msg = format!("message_{}", i);
                        app.chat.input_buffer = msg.clone();
                        app.chat.cursor_pos = msg.len();
                        app.handle_key_event(key(KeyCode::Enter));
                        last_submitted = msg;
                    }

                    // After submitting, input_buffer should be empty
                    assert!(app.chat.input_buffer.is_empty(),
                        "input_buffer should be empty after submit, got: '{}'", app.chat.input_buffer);

                    // Press Alt+Up to recall last submitted message
                    app.handle_key_event(alt_key(KeyCode::Up));

                    // The input_buffer should now contain the last submitted message
                    assert_eq!(
                        app.chat.input_buffer,
                        last_submitted,
                        "Bug: After pressing Alt+Up, input_buffer='{}' but expected last submitted message='{}'. \
                         No history recall mechanism exists.",
                        app.chat.input_buffer, last_submitted
                    );
                });
            }
        }
    }

    // =========================================================================
    // Preservation Property Tests
    // Validates: Requirements 3.1, 3.2, 3.3, 3.4, 3.6
    //
    // These tests capture EXISTING correct behavior on UNFIXED code. They MUST
    // PASS on the current code and continue to pass after the fix is applied,
    // confirming no regressions were introduced.
    // =========================================================================

    mod preservation_properties {
        use super::*;
        use proptest::prelude::*;

        /// **Validates: Requirements 3.2**
        ///
        /// Preservation - Chat Character Insertion
        ///
        /// For any (buffer, cursor_pos, char), after handle_chat_input(Char(c)):
        /// - The buffer contains `c` at the old cursor_pos
        /// - cursor_pos is incremented by 1
        proptest! {
            #[test]
            fn preservation_chat_char_insertion(
                buffer in "[a-zA-Z0-9 ]{0,50}",
                c in proptest::char::range('a', 'z'),
            ) {
                let buf_len = buffer.len();
                // Generate a valid cursor_pos within 0..=buffer.len()
                let cursor_pos_val = if buf_len == 0 { 0 } else { buf_len / 2 };

                let mut app = test_app();
                app.focus = Pane::Chat;
                app.chat.input_buffer = buffer.clone();
                app.chat.cursor_pos = cursor_pos_val;

                let old_cursor = app.chat.cursor_pos;
                let old_buffer = app.chat.input_buffer.clone();

                app.handle_key_event(key(KeyCode::Char(c)));

                // cursor_pos should be incremented by 1
                prop_assert_eq!(
                    app.chat.cursor_pos,
                    old_cursor + 1,
                    "cursor_pos should increment from {} to {} after Char('{}') insertion",
                    old_cursor, old_cursor + 1, c
                );

                // The character should be inserted at the old cursor position
                let chars_vec: Vec<char> = app.chat.input_buffer.chars().collect();
                prop_assert_eq!(
                    chars_vec[old_cursor],
                    c,
                    "Character '{}' should be at position {} in buffer '{}'",
                    c, old_cursor, app.chat.input_buffer
                );

                // Buffer length should increase by 1
                prop_assert_eq!(
                    app.chat.input_buffer.len(),
                    old_buffer.len() + c.len_utf8(),
                    "Buffer length should increase by char byte length"
                );
            }
        }

        /// **Validates: Requirements 3.3**
        ///
        /// Preservation - Chat Enter Submission
        ///
        /// For any non-empty buffer, after handle_chat_input(Enter):
        /// - input_buffer is empty
        /// - cursor_pos is 0
        /// - scroll_offset is 0
        proptest! {
            #[test]
            fn preservation_chat_enter_submission(
                buffer in "[a-zA-Z0-9]{1,50}",
            ) {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    let mut app = test_app();
                    app.focus = Pane::Chat;
                    app.chat.input_buffer = buffer.clone();
                    app.chat.cursor_pos = buffer.len();
                    // Set a non-zero scroll_offset to verify it resets
                    app.chat.scroll_offset = 5;

                    app.handle_key_event(key(KeyCode::Enter));

                    // input_buffer should be cleared
                    assert!(
                        app.chat.input_buffer.is_empty(),
                        "input_buffer should be empty after Enter, got: '{}'",
                        app.chat.input_buffer
                    );

                    // cursor_pos should be reset to 0
                    assert_eq!(
                        app.chat.cursor_pos, 0,
                        "cursor_pos should be 0 after Enter, got: {}",
                        app.chat.cursor_pos
                    );

                    // scroll_offset should be reset to 0
                    assert_eq!(
                        app.chat.scroll_offset, 0,
                        "scroll_offset should be 0 after Enter, got: {}",
                        app.chat.scroll_offset
                    );
                });
            }
        }

        /// **Validates: Requirements 3.1**
        ///
        /// Preservation - File Tree Selection Within Viewport (Up)
        ///
        /// For any file tree where selected_index > 0, after pressing Up:
        /// - selected_index is decremented by 1
        proptest! {
            #[test]
            fn preservation_file_tree_up_decrements(
                entries_len in 2..50usize,
                selected_index_offset in 1..49usize,
            ) {
                let selected_index = selected_index_offset.min(entries_len - 1);
                // Only test when selected_index > 0
                prop_assume!(selected_index > 0);

                let mut app = test_app();
                app.focus = Pane::FileTree;
                app.file_tree.entries = (0..entries_len)
                    .map(|i| FileEntry {
                        path: PathBuf::from(format!("file_{}.txt", i)),
                        name: format!("file_{}.txt", i),
                        is_dir: false,
                        depth: 0,
                    })
                    .collect();
                app.file_tree.selected_index = selected_index;

                let old_selected = app.file_tree.selected_index;
                app.handle_key_event(key(KeyCode::Up));

                prop_assert_eq!(
                    app.file_tree.selected_index,
                    old_selected - 1,
                    "selected_index should decrement from {} to {} after Up",
                    old_selected, old_selected - 1
                );
            }
        }

        /// **Validates: Requirements 3.1**
        ///
        /// Preservation - File Tree Selection Within Viewport (Down)
        ///
        /// For any file tree where selected_index < entries_len - 1, after pressing Down:
        /// - selected_index is incremented by 1
        proptest! {
            #[test]
            fn preservation_file_tree_down_increments(
                entries_len in 2..50usize,
                selected_index_offset in 0..48usize,
            ) {
                let selected_index = selected_index_offset.min(entries_len - 2);

                let mut app = test_app();
                app.focus = Pane::FileTree;
                app.file_tree.entries = (0..entries_len)
                    .map(|i| FileEntry {
                        path: PathBuf::from(format!("file_{}.txt", i)),
                        name: format!("file_{}.txt", i),
                        is_dir: false,
                        depth: 0,
                    })
                    .collect();
                app.file_tree.selected_index = selected_index;

                let old_selected = app.file_tree.selected_index;
                app.handle_key_event(key(KeyCode::Down));

                prop_assert_eq!(
                    app.file_tree.selected_index,
                    old_selected + 1,
                    "selected_index should increment from {} to {} after Down",
                    old_selected, old_selected + 1
                );
            }
        }

        /// **Validates: Requirements 3.4**
        ///
        /// Preservation - Chat Plain Up Scroll Behavior
        ///
        /// For any chat state, after pressing Up (no modifier):
        /// - If scroll_offset > 0: scroll_offset decrements by 1
        /// - If scroll_offset == 0: scroll_offset becomes usize::MAX (sentinel)
        proptest! {
            #[test]
            fn preservation_chat_up_scroll(
                scroll_offset in 0..100usize,
            ) {
                let mut app = test_app();
                app.focus = Pane::Chat;
                app.chat.scroll_offset = scroll_offset;

                app.handle_key_event(key(KeyCode::Up));

                if scroll_offset > 0 {
                    prop_assert_eq!(
                        app.chat.scroll_offset,
                        scroll_offset - 1,
                        "scroll_offset should decrement from {} to {} on Up",
                        scroll_offset, scroll_offset - 1
                    );
                } else {
                    prop_assert_eq!(
                        app.chat.scroll_offset,
                        usize::MAX,
                        "scroll_offset should become usize::MAX sentinel when Up pressed at 0"
                    );
                }
            }
        }

        /// **Validates: Requirements 3.4**
        ///
        /// Preservation - Chat Plain Down Scroll Behavior
        ///
        /// For any chat state with scroll_offset > 0, after pressing Down:
        /// - scroll_offset decrements by 1 (toward 0 = auto-scroll)
        proptest! {
            #[test]
            fn preservation_chat_down_scroll(
                scroll_offset in 1..100usize,
            ) {
                let mut app = test_app();
                app.focus = Pane::Chat;
                app.chat.scroll_offset = scroll_offset;

                app.handle_key_event(key(KeyCode::Down));

                prop_assert_eq!(
                    app.chat.scroll_offset,
                    scroll_offset - 1,
                    "scroll_offset should decrement from {} to {} on Down",
                    scroll_offset, scroll_offset - 1
                );
            }
        }

        /// **Validates: Requirements 3.6**
        ///
        /// Preservation - Chat PageUp Scroll Behavior
        ///
        /// For any chat state, after pressing PageUp:
        /// - If scroll_offset > 5: scroll_offset decrements by 5
        /// - If scroll_offset in 1..=5: scroll_offset becomes 1
        /// - If scroll_offset == 0: scroll_offset becomes usize::MAX (sentinel)
        proptest! {
            #[test]
            fn preservation_chat_pageup_scroll(
                scroll_offset in 0..100usize,
            ) {
                let mut app = test_app();
                app.focus = Pane::Chat;
                app.chat.scroll_offset = scroll_offset;

                app.handle_key_event(key(KeyCode::PageUp));

                if scroll_offset > 5 {
                    prop_assert_eq!(
                        app.chat.scroll_offset,
                        scroll_offset - 5,
                        "scroll_offset should decrement by 5 from {} to {} on PageUp",
                        scroll_offset, scroll_offset - 5
                    );
                } else if scroll_offset > 0 {
                    prop_assert_eq!(
                        app.chat.scroll_offset,
                        1,
                        "scroll_offset should become 1 when PageUp pressed with offset {} in 1..=5",
                        scroll_offset
                    );
                } else {
                    prop_assert_eq!(
                        app.chat.scroll_offset,
                        usize::MAX,
                        "scroll_offset should become usize::MAX sentinel when PageUp pressed at 0"
                    );
                }
            }
        }

        /// **Validates: Requirements 3.6**
        ///
        /// Preservation - Chat PageDown Scroll Behavior
        ///
        /// For any chat state, after pressing PageDown:
        /// - If scroll_offset > 5: scroll_offset decrements by 5
        /// - If scroll_offset <= 5: scroll_offset becomes 0
        proptest! {
            #[test]
            fn preservation_chat_pagedown_scroll(
                scroll_offset in 0..100usize,
            ) {
                let mut app = test_app();
                app.focus = Pane::Chat;
                app.chat.scroll_offset = scroll_offset;

                app.handle_key_event(key(KeyCode::PageDown));

                if scroll_offset > 5 {
                    prop_assert_eq!(
                        app.chat.scroll_offset,
                        scroll_offset - 5,
                        "scroll_offset should decrement by 5 from {} to {} on PageDown",
                        scroll_offset, scroll_offset - 5
                    );
                } else {
                    prop_assert_eq!(
                        app.chat.scroll_offset,
                        0,
                        "scroll_offset should become 0 when PageDown pressed with offset {} <= 5",
                        scroll_offset
                    );
                }
            }
        }
    }
}
