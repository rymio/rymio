// RAG store - persistence layer for indexed documents

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::tfidf::TfIdfIndex;
use super::RagError;

/// The type of a document stored in the RAG index.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DocType {
    /// A file or directory entry from the project tree.
    TreeEntry,
    /// A chunk of file content that has been indexed.
    ContentChunk,
}

/// Metadata associated with an indexed document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    pub file_name: String,
    pub relative_path: String,
    pub parent_dir: String,
    pub is_directory: bool,
    pub line_start: Option<usize>,
    pub line_end: Option<usize>,
    pub chunk_index: Option<usize>,
}

/// A document stored in the RAG index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: u64,
    pub path: PathBuf,
    pub doc_type: DocType,
    pub content: String,
    pub metadata: DocumentMetadata,
    pub embedding: Option<Vec<f32>>,
}

/// A single search result returned from a RAG query.
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub document_id: u64,
    pub path: PathBuf,
    pub content: String,
    pub score: f64,
    pub metadata: DocumentMetadata,
}

/// The on-disk JSON format for the RAG store.
#[derive(Debug, Serialize, Deserialize)]
struct StoreFile {
    version: u32,
    documents: Vec<Document>,
}

/// The persistence layer that holds indexed documents and their embeddings/TF-IDF vectors.
pub struct RagStore {
    pub(crate) documents: Vec<Document>,
    pub(crate) tfidf_index: TfIdfIndex,
    pub(crate) db_path: PathBuf,
}

impl RagStore {
    /// Loads an existing store from `db_path`, or creates a new empty store if the file
    /// is missing or corrupted.
    pub fn load_or_create(db_path: &Path) -> Result<Self, RagError> {
        let path_buf = db_path.to_path_buf();

        if !db_path.exists() {
            return Ok(Self {
                documents: Vec::new(),
                tfidf_index: TfIdfIndex::new(),
                db_path: path_buf,
            });
        }

        let data = match fs::read_to_string(db_path) {
            Ok(d) => d,
            Err(_) => {
                // File unreadable — return empty store
                return Ok(Self {
                    documents: Vec::new(),
                    tfidf_index: TfIdfIndex::new(),
                    db_path: path_buf,
                });
            }
        };

        let store_file: StoreFile = match serde_json::from_str(&data) {
            Ok(sf) => sf,
            Err(_) => {
                // Corrupted JSON — return empty store
                return Ok(Self {
                    documents: Vec::new(),
                    tfidf_index: TfIdfIndex::new(),
                    db_path: path_buf,
                });
            }
        };

        // Rebuild TF-IDF index from loaded documents
        let mut tfidf_index = TfIdfIndex::new();
        let contents: Vec<&str> = store_file
            .documents
            .iter()
            .map(|d| d.content.as_str())
            .collect();
        tfidf_index.rebuild(&contents);

        Ok(Self {
            documents: store_file.documents,
            tfidf_index,
            db_path: path_buf,
        })
    }

    /// Serializes the store to JSON and writes it to `db_path`.
    /// Creates the parent directory (`.litecode`) if it does not exist.
    pub fn save(&self) -> Result<(), RagError> {
        // Ensure parent directory exists
        if let Some(parent) = self.db_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        let store_file = StoreFile {
            version: 1,
            documents: self.documents.clone(),
        };

        let json = serde_json::to_string_pretty(&store_file)
            .map_err(|e| RagError::SerializationError(e.to_string()))?;

        fs::write(&self.db_path, json)?;

        Ok(())
    }

    /// Inserts a document into the store, assigning the next available ID
    /// and updating the TF-IDF index.
    pub fn insert(&mut self, mut doc: Document) {
        let next_id = self.documents.iter().map(|d| d.id).max().unwrap_or(0) + 1;
        doc.id = next_id;

        let doc_index = self.documents.len();
        self.tfidf_index.add_document(doc_index, &doc.content);
        self.documents.push(doc);
    }

    /// Removes all documents whose path matches the given path,
    /// then rebuilds the TF-IDF index from remaining documents.
    pub fn remove_by_path(&mut self, path: &Path) {
        self.documents.retain(|d| d.path != path);

        // Rebuild TF-IDF index from remaining documents
        let contents: Vec<&str> = self.documents.iter().map(|d| d.content.as_str()).collect();
        self.tfidf_index.rebuild(&contents);
    }

    /// Performs a TF-IDF search and returns the top_k results as SearchHits.
    pub fn search_tfidf(&self, query: &str, top_k: usize) -> Vec<SearchHit> {
        let results = self.tfidf_index.query(query, top_k);

        results
            .into_iter()
            .filter_map(|(doc_index, score)| {
                self.documents.get(doc_index).map(|doc| SearchHit {
                    document_id: doc.id,
                    path: doc.path.clone(),
                    content: doc.content.clone(),
                    score,
                    metadata: doc.metadata.clone(),
                })
            })
            .collect()
    }

    /// Performs an embedding-based search using cosine similarity.
    /// Returns the top_k results with score > 0, sorted by score descending.
    pub fn search_embedding(&self, query_embedding: &[f32], top_k: usize) -> Vec<SearchHit> {
        let query_magnitude = vector_magnitude(query_embedding);
        if query_magnitude == 0.0 {
            return Vec::new();
        }

        let mut scored: Vec<(usize, f64)> = self
            .documents
            .iter()
            .enumerate()
            .filter_map(|(idx, doc)| {
                let embedding = doc.embedding.as_ref()?;
                let doc_magnitude = vector_magnitude(embedding);
                if doc_magnitude == 0.0 {
                    return None;
                }
                let dot: f64 = query_embedding
                    .iter()
                    .zip(embedding.iter())
                    .map(|(a, b)| (*a as f64) * (*b as f64))
                    .sum();
                let similarity = dot / (query_magnitude * doc_magnitude);
                if similarity > 0.0 {
                    Some((idx, similarity))
                } else {
                    None
                }
            })
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);

        scored
            .into_iter()
            .filter_map(|(idx, score)| {
                self.documents.get(idx).map(|doc| SearchHit {
                    document_id: doc.id,
                    path: doc.path.clone(),
                    content: doc.content.clone(),
                    score,
                    metadata: doc.metadata.clone(),
                })
            })
            .collect()
    }

    /// Returns true if any ContentChunk document exists for the given path.
    pub fn has_content_for(&self, path: &Path) -> bool {
        self.documents
            .iter()
            .any(|d| d.doc_type == DocType::ContentChunk && d.path == path)
    }

    /// Collects all unique paths from documents in the store.
    pub fn get_all_paths(&self) -> HashSet<PathBuf> {
        self.documents.iter().map(|d| d.path.clone()).collect()
    }

    /// Returns true if a TreeEntry document already exists for the given path.
    pub fn has_tree_entry(&self, path: &Path) -> bool {
        self.documents
            .iter()
            .any(|d| d.doc_type == DocType::TreeEntry && d.path == path)
    }

    /// Returns a HashSet of paths from TreeEntry documents only.
    pub fn get_tree_entry_paths(&self) -> HashSet<PathBuf> {
        self.documents
            .iter()
            .filter(|d| d.doc_type == DocType::TreeEntry)
            .map(|d| d.path.clone())
            .collect()
    }

    /// Removes only ContentChunk documents for the given path,
    /// then rebuilds the TF-IDF index from remaining documents.
    pub fn remove_content_chunks_by_path(&mut self, path: &Path) {
        self.documents
            .retain(|d| !(d.doc_type == DocType::ContentChunk && d.path == path));

        // Rebuild TF-IDF index from remaining documents
        let contents: Vec<&str> = self.documents.iter().map(|d| d.content.as_str()).collect();
        self.tfidf_index.rebuild(&contents);
    }
}

/// Computes the L2 magnitude of a float vector.
fn vector_magnitude(v: &[f32]) -> f64 {
    v.iter()
        .map(|x| (*x as f64) * (*x as f64))
        .sum::<f64>()
        .sqrt()
}
