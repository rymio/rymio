use std::path::PathBuf;

/// File extensions for compiled/binary files that should not be content-indexed.
pub const COMPILED_EXTENSIONS: &[&str] = &[
    "o", "so", "dll", "class", "pyc", "wasm", "exe", "a", "lib", "obj", "beam",
    "dSYM", "pdb", "jar", "war", "ear",
];

/// Configuration for the RAG subsystem.
#[derive(Debug, Clone)]
pub struct RagConfig {
    /// Whether RAG indexing and retrieval is enabled.
    pub enabled: bool,
    /// Path to the persistent JSON store file.
    pub db_path: PathBuf,
    /// Number of lines per content chunk.
    pub chunk_lines: usize,
    /// Number of overlapping lines between consecutive chunks.
    pub overlap_lines: usize,
    /// Maximum number of results to return from a query.
    pub top_k: usize,
    /// List of file extensions considered compiled/binary (not content-indexed).
    pub compiled_extensions: Vec<String>,
}

impl Default for RagConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            db_path: PathBuf::from(".litecode/rag_index.json"),
            chunk_lines: 50,
            overlap_lines: 10,
            top_k: 5,
            compiled_extensions: COMPILED_EXTENSIONS
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }
}
