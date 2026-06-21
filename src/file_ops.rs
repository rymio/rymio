use std::fs;
use std::path::{Path, PathBuf};

use glob::Pattern;

use crate::tools::{read_file_safe, validate_file_op_path};

/// Result of a file operation handler, used to drive UI updates.
#[derive(Debug)]
pub struct FileOpResult {
    /// Whether the operation succeeded.
    pub success: bool,
    /// Human-readable message for the Chat Pane.
    pub message: String,
    /// Path to navigate to in the file tree (expand parents + select).
    pub navigate_to: Option<PathBuf>,
    /// Path to open in the editor pane.
    pub open_file: Option<PathBuf>,
    /// Whether the file tree needs a full refresh.
    pub refresh_tree: bool,
}

/// Pending file creation details awaiting overwrite confirmation.
#[derive(Debug)]
pub struct PendingCreate {
    pub path: PathBuf,
    pub content: String,
}

/// Start a file creation operation.
///
/// Validates the target location and checks for existing files:
/// - Resolves target path as `selected_dir/filename`
/// - Validates the parent directory via `validate_file_op_path`
/// - If file already exists: returns failure with overwrite warning and a `PendingCreate`
/// - If file doesn't exist: returns success with "Generating content..." message
///
/// The actual LLM content generation is NOT triggered here — the app event loop
/// handles sending the LLM request when it sees a successful FileCreate result.
pub fn handle_create_start(
    filename: &str,
    _description: &str,
    selected_dir: &Path,
    root: &Path,
) -> (FileOpResult, Option<PendingCreate>) {
    let target_path = selected_dir.join(filename);

    // Validate the parent directory of the target path
    let parent_dir = target_path.parent().unwrap_or(selected_dir);

    // Convert parent to a string relative to root for validation
    let parent_str = parent_dir
        .strip_prefix(root)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| parent_dir.to_string_lossy().to_string());

    // Use "." if the parent is the root itself
    let parent_str = if parent_str.is_empty() {
        ".".to_string()
    } else {
        parent_str
    };

    if let Err(err) = validate_file_op_path(&parent_str, root) {
        return (
            FileOpResult {
                success: false,
                message: err,
                navigate_to: None,
                open_file: None,
                refresh_tree: false,
            },
            None,
        );
    }

    // Check if the file already exists
    if target_path.exists() {
        let pending = PendingCreate {
            path: target_path.clone(),
            content: String::new(), // Content will be filled after LLM generation
        };
        (
            FileOpResult {
                success: false,
                message: format!("File '{}' already exists. Overwrite? (y/n)", filename),
                navigate_to: None,
                open_file: None,
                refresh_tree: false,
            },
            Some(pending),
        )
    } else {
        (
            FileOpResult {
                success: true,
                message: format!("Generating content for '{}'...", filename),
                navigate_to: None,
                open_file: None,
                refresh_tree: false,
            },
            None,
        )
    }
}

/// Navigate to a path in the file tree.
///
/// Validates the path, then returns a `FileOpResult` indicating:
/// - For directories: navigate_to is set to the directory path
/// - For files: navigate_to is set to the parent directory, open_file is set to the file path
/// - For non-existent paths: returns an error with "not found" message
/// - For invalid paths (security violations): returns the validation error
pub fn handle_navigate(path: &str, root: &Path) -> FileOpResult {
    // Validate the path using the path safety module
    let validated_path = match validate_file_op_path(path, root) {
        Ok(p) => p,
        Err(err) => {
            return FileOpResult {
                success: false,
                message: err,
                navigate_to: None,
                open_file: None,
                refresh_tree: false,
            };
        }
    };

    // Check if the path exists and determine its type
    if validated_path.is_dir() {
        FileOpResult {
            success: true,
            message: format!("Navigated to: '{}'", validated_path.display()),
            navigate_to: Some(validated_path),
            open_file: None,
            refresh_tree: false,
        }
    } else if validated_path.is_file() {
        let parent = validated_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| root.to_path_buf());
        FileOpResult {
            success: true,
            message: format!("Opened file: '{}'", validated_path.display()),
            navigate_to: Some(parent),
            open_file: Some(validated_path),
            refresh_tree: false,
        }
    } else {
        // Path doesn't exist
        FileOpResult {
            success: false,
            message: format!("Path not found: '{}'", path),
            navigate_to: None,
            open_file: None,
            refresh_tree: false,
        }
    }
}

/// Complete a file creation operation by writing content to disk.
///
/// Called when the LLM content generation finishes:
/// - Writes `content` to the file at `path` using `std::fs::write`
/// - On success: returns `FileOpResult` with `refresh_tree=true` and `open_file` set
/// - On failure: returns descriptive error (permission denied, disk full, etc.)
pub fn handle_create_complete(path: &Path, content: &str) -> FileOpResult {
    match fs::write(path, content) {
        Ok(()) => FileOpResult {
            success: true,
            message: format!("Created file: '{}'", path.display()),
            navigate_to: None,
            open_file: Some(path.to_path_buf()),
            refresh_tree: true,
        },
        Err(e) => {
            let message = match e.kind() {
                std::io::ErrorKind::PermissionDenied => {
                    format!("Permission denied: cannot write to '{}'.", path.display())
                }
                _ if e.to_string().contains("No space left on device") => {
                    "Disk full: cannot write file.".to_string()
                }
                _ => {
                    format!("Failed to create file '{}': {}", path.display(), e)
                }
            };
            FileOpResult {
                success: false,
                message,
                navigate_to: None,
                open_file: None,
                refresh_tree: false,
            }
        }
    }
}

/// Start a file edit operation.
///
/// Validates the target path and reads the file content:
/// - Calls `validate_file_op_path` to ensure the path is safe
/// - If the file doesn't exist: returns an error suggesting creation
/// - If the file exists: reads content using `read_file_safe` (300 KB limit)
/// - Returns the file content along with an "Editing file..." status message
///
/// The actual LLM call is NOT made here — this function validates and reads the file.
/// The app event loop handles sending the LLM request with the file content and instruction.
///
/// # Returns
/// - `Ok((FileOpResult, file_content))` when the file exists and can be read
/// - `Err(FileOpResult)` when validation fails or file doesn't exist
pub fn handle_edit_start(
    path: &str,
    _instruction: &str,
    root: &Path,
) -> Result<(FileOpResult, String), FileOpResult> {
    // Validate the path using the path safety module
    let validated_path = match validate_file_op_path(path, root) {
        Ok(p) => p,
        Err(err) => {
            return Err(FileOpResult {
                success: false,
                message: err,
                navigate_to: None,
                open_file: None,
                refresh_tree: false,
            });
        }
    };

    // Check if the file exists
    if !validated_path.exists() || validated_path.is_dir() {
        return Err(FileOpResult {
            success: false,
            message: format!("File not found: '{}'. Would you like to create it?", path),
            navigate_to: None,
            open_file: None,
            refresh_tree: false,
        });
    }

    // Read the file content
    let content = match read_file_safe(&validated_path, 300) {
        Ok(c) => c,
        Err(err) => {
            return Err(FileOpResult {
                success: false,
                message: err,
                navigate_to: None,
                open_file: None,
                refresh_tree: false,
            });
        }
    };

    Ok((
        FileOpResult {
            success: true,
            message: format!("Editing file '{}'...", path),
            navigate_to: None,
            open_file: None,
            refresh_tree: false,
        },
        content,
    ))
}

/// Find files matching a glob pattern within the working directory.
///
/// Walks the root directory recursively, matching filenames against the glob pattern:
/// - Parses the pattern using `glob::Pattern`
/// - Skips directories whose name is in `ignored_dirs`
/// - Matches each file's name (not full path) against the pattern
/// - Returns a numbered list of relative paths on success
/// - Returns a suggestion message if no matches are found
pub fn handle_find(pattern: &str, root: &Path, ignored_dirs: &[String]) -> FileOpResult {
    // Parse the glob pattern
    let glob_pattern = match Pattern::new(pattern) {
        Ok(p) => p,
        Err(e) => {
            return FileOpResult {
                success: false,
                message: format!("Invalid glob pattern '{}': {}", pattern, e),
                navigate_to: None,
                open_file: None,
                refresh_tree: false,
            };
        }
    };

    // Recursively walk the directory and collect matches
    let mut matches: Vec<PathBuf> = Vec::new();
    walk_directory(root, root, &glob_pattern, ignored_dirs, &mut matches);

    if matches.is_empty() {
        FileOpResult {
            success: false,
            message: format!(
                "No files matching '{}' found. Try a broader pattern like '*.rs' or '*config*'.",
                pattern
            ),
            navigate_to: None,
            open_file: None,
            refresh_tree: false,
        }
    } else {
        // Sort matches for consistent output
        matches.sort();

        // Build numbered list of relative paths
        let numbered_list: String = matches
            .iter()
            .enumerate()
            .map(|(i, path)| format!("{}. {}", i + 1, path.display()))
            .collect::<Vec<_>>()
            .join("\n");

        FileOpResult {
            success: true,
            message: numbered_list,
            navigate_to: None,
            open_file: None,
            refresh_tree: false,
        }
    }
}

/// Recursively walk a directory, collecting files whose names match the glob pattern.
/// Skips directories whose name is in `ignored_dirs`.
fn walk_directory(
    dir: &Path,
    root: &Path,
    pattern: &Pattern,
    ignored_dirs: &[String],
    matches: &mut Vec<PathBuf>,
) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };

        if path.is_dir() {
            // Skip ignored directories
            if ignored_dirs.contains(&file_name) {
                continue;
            }
            walk_directory(&path, root, pattern, ignored_dirs, matches);
        } else {
            // Match the filename against the glob pattern
            if pattern.matches(&file_name) {
                // Store relative path from root
                if let Ok(relative) = path.strip_prefix(root) {
                    matches.push(relative.to_path_buf());
                }
            }
        }
    }
}
