pub mod chunker;
pub mod config;
pub mod embedding;
pub mod store;
pub mod tfidf;

pub use store::{DocType, Document, DocumentMetadata, RagStore, SearchHit};

use std::collections::HashSet;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use chunker::{chunk_file_content, ChunkConfig};
use config::RagConfig;
use embedding::EmbeddingMode;

/// Represents a file or directory entry from the project tree.
#[derive(Debug, Clone)]
pub struct TreeEntry {
    pub path: PathBuf,
    pub is_directory: bool,
}

/// Errors that can occur within the RAG subsystem.
#[derive(Debug)]
pub enum RagError {
    IoError(io::Error),
    SerializationError(String),
    EmbeddingError(String),
    StoreCorrupted(String),
    ConfigError(String),
}

impl fmt::Display for RagError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "RAG I/O error: {e}"),
            Self::SerializationError(msg) => write!(f, "RAG serialization error: {msg}"),
            Self::EmbeddingError(msg) => write!(f, "RAG embedding error: {msg}"),
            Self::StoreCorrupted(msg) => write!(f, "RAG store corrupted: {msg}"),
            Self::ConfigError(msg) => write!(f, "RAG config error: {msg}"),
        }
    }
}

impl From<io::Error> for RagError {
    fn from(err: io::Error) -> Self {
        RagError::IoError(err)
    }
}

/// The central coordinator that owns the RAG store and orchestrates indexing and querying.
pub struct RagManager {
    store: RagStore,
    config: RagConfig,
    embedding_mode: EmbeddingMode,
    is_indexing: bool,
}

impl RagManager {
    /// Creates a new RagManager.
    ///
    /// Determines the database path from config (falling back to `project_root/.litecode/rag_index.json`),
    /// loads or creates the store, and defaults to TF-IDF embedding mode.
    pub fn new(config: RagConfig, project_root: &Path) -> Result<Self, RagError> {
        let db_path = if config.db_path.as_os_str().is_empty() {
            project_root.join(".litecode/rag_index.json")
        } else if config.db_path.is_relative() {
            project_root.join(&config.db_path)
        } else {
            config.db_path.clone()
        };

        let store = RagStore::load_or_create(&db_path)?;

        Ok(Self {
            store,
            config,
            embedding_mode: EmbeddingMode::TfIdf,
            is_indexing: false,
        })
    }

    /// Returns whether RAG is enabled in the configuration.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Returns the configured top_k value for query results.
    pub fn top_k(&self) -> usize {
        self.config.top_k
    }

    /// Persists the store to disk.
    pub fn save(&self) -> Result<(), RagError> {
        self.store.save()
    }

    /// Indexes tree entries into the store.
    ///
    /// For each entry:
    /// - Skips entries whose path contains a segment matching any of `ignored_dirs`
    /// - Skips entries that already have a TreeEntry document in the store
    /// - Creates a Document with DocType::TreeEntry, content = "{file_name} - {relative_path}"
    /// - Does NOT create ContentChunk documents
    ///
    /// Returns the number of newly indexed entries.
    pub fn index_tree_entries(&mut self, entries: &[TreeEntry], ignored_dirs: &[String]) -> usize {
        let mut indexed_count = 0;

        for entry in entries {
            // Skip entries whose path contains an ignored directory segment
            if self.path_contains_ignored_dir(&entry.path, ignored_dirs) {
                continue;
            }

            // Skip entries already in the store
            if self.store.has_tree_entry(&entry.path) {
                continue;
            }

            // Extract metadata from path components
            let file_name = entry
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let relative_path = entry.path.to_string_lossy().to_string();
            let parent_dir = entry
                .path
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            // Build rich content for TF-IDF matching:
            // Include file name, full path, parent dir, extension, and all path components
            let extension = entry
                .path
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();
            let path_components: Vec<String> = entry
                .path
                .components()
                .filter_map(|c| {
                    if let std::path::Component::Normal(s) = c {
                        Some(s.to_string_lossy().to_string())
                    } else {
                        None
                    }
                })
                .collect();
            let entry_type = if entry.is_directory {
                "directory"
            } else {
                "file"
            };

            let content = format!(
                "{} {} located at {} in directory {} extension {} path components: {}",
                file_name,
                entry_type,
                relative_path,
                parent_dir,
                extension,
                path_components.join(" ")
            );

            let doc = Document {
                id: 0, // Will be assigned by store.insert()
                path: entry.path.clone(),
                doc_type: DocType::TreeEntry,
                content,
                metadata: DocumentMetadata {
                    file_name,
                    relative_path,
                    parent_dir,
                    is_directory: entry.is_directory,
                    line_start: None,
                    line_end: None,
                    chunk_index: None,
                },
                embedding: None,
            };

            self.store.insert(doc);
            indexed_count += 1;
        }

        indexed_count
    }

    /// Checks whether any path component matches an ignored directory name.
    fn path_contains_ignored_dir(&self, path: &Path, ignored_dirs: &[String]) -> bool {
        for component in path.components() {
            if let std::path::Component::Normal(segment) = component {
                let segment_str = segment.to_string_lossy();
                if ignored_dirs.iter().any(|dir| dir == segment_str.as_ref()) {
                    return true;
                }
            }
        }
        false
    }

    /// Indexes the content of a file into the store.
    ///
    /// Steps:
    /// 1. Check if the file extension is in `compiled_extensions` — if so, skip (return 0)
    /// 2. Check if content length exceeds `max_file_kb * 1024` — if so, skip (return 0)
    /// 3. Remove existing ContentChunk documents for this path (handles re-index on save)
    /// 4. Chunk the content using the configured chunk/overlap settings
    /// 5. Create a Document with DocType::ContentChunk for each chunk
    /// 6. Insert each document into the store
    /// 7. Return the number of chunks indexed
    ///
    /// Requirements: 3.1, 3.2, 3.3, 3.4, 4.1, 4.2, 4.3
    pub fn index_file_content(&mut self, path: &Path, content: &str, max_file_kb: u32) -> usize {
        // Check if the file has a compiled extension — skip
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext_lower = ext.to_lowercase();
            if self
                .config
                .compiled_extensions
                .iter()
                .any(|ce| ce == &ext_lower)
            {
                // Compiled file: skip content indexing (tree entry only)
                return 0;
            }
        }

        // Check if content exceeds max_file_kb size limit — skip (tree entry only)
        let max_bytes = (max_file_kb as usize) * 1024;
        if content.len() > max_bytes {
            // Oversized file: skip content indexing (tree entry only)
            return 0;
        }

        // Remove existing ContentChunk documents for this path (handles re-index on save)
        self.store.remove_content_chunks_by_path(path);

        // Chunk the content
        let chunk_config = ChunkConfig {
            chunk_lines: self.config.chunk_lines,
            overlap_lines: self.config.overlap_lines,
        };
        let chunks = chunk_file_content(content, &chunk_config);

        // Extract metadata components from path
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let relative_path = path.to_string_lossy().to_string();
        let parent_dir = path
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // Create and insert a Document for each chunk
        let chunk_count = chunks.len();
        for chunk in chunks {
            let doc = Document {
                id: 0, // Will be assigned by store.insert()
                path: path.to_path_buf(),
                doc_type: DocType::ContentChunk,
                content: chunk.content,
                metadata: DocumentMetadata {
                    file_name: file_name.clone(),
                    relative_path: relative_path.clone(),
                    parent_dir: parent_dir.clone(),
                    is_directory: false,
                    line_start: Some(chunk.line_start),
                    line_end: Some(chunk.line_end),
                    chunk_index: Some(chunk.chunk_index),
                },
                embedding: None, // TF-IDF mode doesn't need embeddings
            };

            self.store.insert(doc);
        }

        chunk_count
    }

    /// Performs a search query against the RAG store.
    ///
    /// If RAG is disabled, returns an empty Vec immediately.
    /// Otherwise, performs a TF-IDF search (used for both TfIdf and Api modes
    /// since the synchronous query method cannot embed the query text).
    ///
    /// Requirements: 5.1, 6.2
    pub fn query(&self, query_text: &str, top_k: usize) -> Vec<SearchHit> {
        if !self.is_enabled() {
            return Vec::new();
        }

        // For both embedding modes, use TF-IDF for the synchronous query.
        // Embedding-based search would require async to embed the query text first.
        self.store.search_tfidf(query_text, top_k)
    }

    /// Reconciles the tree index with the current filesystem state.
    ///
    /// Removes TreeEntry documents for paths that no longer exist in `current_paths`.
    /// New entries are expected to be added via `index_tree_entries()` separately.
    ///
    /// Requirements: 7.3
    pub fn reconcile_tree(&mut self, current_paths: &HashSet<PathBuf>) {
        let stored_paths = self.store.get_tree_entry_paths();

        // Find paths in store but NOT in current_paths → stale, remove them
        let stale_paths: Vec<PathBuf> = stored_paths.difference(current_paths).cloned().collect();

        for path in &stale_paths {
            self.store.remove_by_path(path);
        }
    }

    /// Removes all documents (both TreeEntry and ContentChunk) for the given paths.
    ///
    /// Requirements: 7.3
    pub fn remove_entries(&mut self, paths: &[PathBuf]) {
        for path in paths {
            self.store.remove_by_path(path);
        }
    }
}
