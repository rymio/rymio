use std::fs;
use std::path::PathBuf;

const MAX_HISTORY_ENTRIES: usize = 500;

/// Returns the path to the chat history file: `~/.config/litecode-agent/chat_history.json`
pub fn history_file_path() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|home| {
            PathBuf::from(home)
                .join(".config")
                .join("litecode-agent")
                .join("chat_history.json")
        })
}

/// Load chat history from disk. Returns an empty vec on any error.
/// Caps at 500 entries (keeps the most recent).
pub fn load_chat_history() -> Vec<String> {
    let path = match history_file_path() {
        Some(p) => p,
        None => return Vec::new(),
    };

    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let entries: Vec<String> = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    // Cap at MAX_HISTORY_ENTRIES, keeping the most recent
    if entries.len() > MAX_HISTORY_ENTRIES {
        entries[entries.len() - MAX_HISTORY_ENTRIES..].to_vec()
    } else {
        entries
    }
}

/// Save chat history to disk. Creates parent directories if needed.
/// Caps at 500 entries (keeps the most recent).
pub fn save_chat_history(history: &[String]) -> Result<(), String> {
    let path = match history_file_path() {
        Some(p) => p,
        None => return Err("Cannot determine home directory".to_string()),
    };

    // Cap at MAX_HISTORY_ENTRIES, keeping the most recent
    let to_save = if history.len() > MAX_HISTORY_ENTRIES {
        &history[history.len() - MAX_HISTORY_ENTRIES..]
    } else {
        history
    };

    // Create parent directories if needed
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
    }

    let json = serde_json::to_string_pretty(to_save)
        .map_err(|e| format!("Failed to serialize history: {}", e))?;

    fs::write(&path, json)
        .map_err(|e| format!("Failed to write history to {}: {}", path.display(), e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_history_file_path_returns_expected_path() {
        // This test depends on HOME being set
        if std::env::var("HOME").is_ok() {
            let path = history_file_path();
            assert!(path.is_some());
            let p = path.unwrap();
            assert!(p.ends_with("chat_history.json"));
            assert!(p.to_string_lossy().contains(".config/litecode-agent"));
        }
    }

    /// Helper: write history to a specific path and read it back, bypassing HOME env var.
    fn save_to_path(path: &std::path::Path, history: &[String]) -> Result<(), String> {
        let to_save = if history.len() > MAX_HISTORY_ENTRIES {
            &history[history.len() - MAX_HISTORY_ENTRIES..]
        } else {
            history
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        }
        let json = serde_json::to_string_pretty(to_save)
            .map_err(|e| format!("Failed to serialize: {}", e))?;
        fs::write(path, json)
            .map_err(|e| format!("Failed to write: {}", e))?;
        Ok(())
    }

    fn load_from_path(path: &std::path::Path) -> Vec<String> {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let entries: Vec<String> = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        if entries.len() > MAX_HISTORY_ENTRIES {
            entries[entries.len() - MAX_HISTORY_ENTRIES..].to_vec()
        } else {
            entries
        }
    }

    #[test]
    fn test_load_missing_file_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent.json");
        let history = load_from_path(&path);
        assert!(history.is_empty());
    }

    #[test]
    fn test_save_and_load_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("chat_history.json");

        let entries = vec![
            "hello".to_string(),
            "world".to_string(),
            "test command".to_string(),
        ];

        let result = save_to_path(&path, &entries);
        assert!(result.is_ok());

        let loaded = load_from_path(&path);
        assert_eq!(loaded, entries);
    }

    #[test]
    fn test_save_caps_at_500_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("chat_history.json");

        let entries: Vec<String> = (0..600).map(|i| format!("entry {}", i)).collect();

        let result = save_to_path(&path, &entries);
        assert!(result.is_ok());

        let loaded = load_from_path(&path);
        assert_eq!(loaded.len(), 500);
        // Should keep the most recent 500 (entries 100..600)
        assert_eq!(loaded[0], "entry 100");
        assert_eq!(loaded[499], "entry 599");
    }

    #[test]
    fn test_load_caps_at_500_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("chat_history.json");

        // Write more than 500 entries directly to file
        let entries: Vec<String> = (0..700).map(|i| format!("item {}", i)).collect();
        let json = serde_json::to_string(&entries).unwrap();
        fs::write(&path, json).unwrap();

        let loaded = load_from_path(&path);
        assert_eq!(loaded.len(), 500);
        // Should keep the most recent 500 (items 200..700)
        assert_eq!(loaded[0], "item 200");
        assert_eq!(loaded[499], "item 699");
    }

    #[test]
    fn test_load_invalid_json_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("chat_history.json");
        fs::write(&path, "not valid json {{{").unwrap();

        let loaded = load_from_path(&path);
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_save_creates_parent_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested").join("dir").join("chat_history.json");

        let entries = vec!["test".to_string()];
        let result = save_to_path(&path, &entries);
        assert!(result.is_ok());

        // Verify the file exists
        assert!(path.exists());
    }
}
