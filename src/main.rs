use std::io;
use std::path::PathBuf;
use std::process;

use clap::Parser;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

mod app;
mod config;
mod file_ops;
mod history;
mod llm;
mod tasks;
mod patches;
mod tools;
mod prompts;
mod rag;
mod ui;

/// litecode-agent — A TUI coding assistant
#[derive(Parser, Debug)]
#[command(name = "litecode-agent", version, about)]
struct Cli {
    /// Project root directory
    #[arg(value_name = "ROOT")]
    root_positional: Option<PathBuf>,

    /// Project root directory (alternative to positional argument)
    #[arg(long = "root", value_name = "DIR")]
    root_flag: Option<PathBuf>,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Determine root directory: --root flag takes precedence, then positional, then cwd
    let root = cli
        .root_flag
        .or(cli.root_positional)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // Validate root exists
    if !root.exists() {
        eprintln!("Error: '{}' does not exist.", root.display());
        process::exit(1);
    }

    // Validate root is a directory
    if !root.is_dir() {
        eprintln!("Error: '{}' is not a directory.", root.display());
        process::exit(1);
    }

    // Load configuration
    let config = config::load_config(&root);

    // Set up terminal
    enable_raw_mode().expect("Failed to enable raw mode");
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).expect("Failed to enter alternate screen");
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).expect("Failed to create terminal");

    // Create app and run
    let mut app = app::App::new(config, root);
    let result = app.run(&mut terminal).await;

    // Restore terminal
    disable_raw_mode().expect("Failed to disable raw mode");
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .expect("Failed to leave alternate screen");
    terminal.show_cursor().expect("Failed to show cursor");

    if let Err(e) = result {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}
