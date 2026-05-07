use std::collections::HashMap;

/// A sparse vector representation for TF-IDF document vectors.
#[derive(Debug, Clone)]
pub struct SparseVector {
    pub indices: Vec<usize>,
    pub values: Vec<f64>,
}

/// A lightweight TF-IDF index for keyword-based similarity search.
pub struct TfIdfIndex {
    vocabulary: HashMap<String, usize>,
    idf_scores: Vec<f64>,
    doc_vectors: Vec<SparseVector>,
}

/// Common English stopwords to filter during tokenization.
const STOPWORDS: &[&str] = &[
    "a", "an", "the", "is", "are", "was", "were", "be", "been", "being",
    "have", "has", "had", "do", "does", "did", "will", "would", "could",
    "should", "may", "might", "shall", "can", "need", "dare", "ought",
    "used", "to", "of", "in", "for", "on", "with", "at", "by", "from",
    "as", "into", "through", "during", "before", "after", "above", "below",
    "between", "out", "off", "over", "under", "again", "further", "then",
    "once", "and", "but", "or", "nor", "not", "so", "yet", "both",
    "either", "neither", "each", "every", "all", "any", "few", "more",
    "most", "other", "some", "such", "no", "only", "own", "same", "than",
    "too", "very", "just", "because", "if", "when", "where", "how", "what",
    "which", "who", "whom", "this", "that", "these", "those", "it", "its",
    "i", "me", "my", "we", "our", "you", "your", "he", "him", "his",
    "she", "her", "they", "them", "their",
];

impl TfIdfIndex {
    /// Creates a new empty TF-IDF index.
    pub fn new() -> Self {
        Self {
            vocabulary: HashMap::new(),
            idf_scores: Vec::new(),
            doc_vectors: Vec::new(),
        }
    }

    /// Rebuilds the entire index from a slice of document content strings.
    ///
    /// This recomputes the vocabulary, IDF scores, and all document vectors.
    pub fn rebuild(&mut self, documents: &[&str]) {
        self.vocabulary.clear();
        self.idf_scores.clear();
        self.doc_vectors.clear();

        if documents.is_empty() {
            return;
        }

        // First pass: build vocabulary and document frequency counts
        let mut doc_freq: HashMap<String, usize> = HashMap::new();
        let tokenized_docs: Vec<Vec<String>> = documents
            .iter()
            .map(|doc| Self::tokenize(doc))
            .collect();

        for tokens in &tokenized_docs {
            let mut seen_in_doc: HashMap<&str, bool> = HashMap::new();
            for token in tokens {
                if !seen_in_doc.contains_key(token.as_str()) {
                    seen_in_doc.insert(token.as_str(), true);
                    *doc_freq.entry(token.clone()).or_insert(0) += 1;
                }
            }
        }

        // Assign vocabulary indices
        let mut vocab_list: Vec<String> = doc_freq.keys().cloned().collect();
        vocab_list.sort(); // deterministic ordering
        for (idx, term) in vocab_list.iter().enumerate() {
            self.vocabulary.insert(term.clone(), idx);
        }

        // Compute IDF scores
        let num_docs = documents.len() as f64;
        self.idf_scores = vocab_list
            .iter()
            .map(|term| {
                let df = *doc_freq.get(term).unwrap_or(&0) as f64;
                (num_docs / (1.0 + df)).ln()
            })
            .collect();

        // Second pass: build TF-IDF vectors for each document
        for tokens in &tokenized_docs {
            let vector = self.compute_tfidf_vector(tokens);
            self.doc_vectors.push(vector);
        }
    }

    /// Adds a single document to the index.
    ///
    /// The `doc_id` should correspond to the index in the document collection.
    /// New terms in the document are added to the vocabulary and IDF scores
    /// are recomputed for affected terms.
    pub fn add_document(&mut self, _doc_id: usize, text: &str) {
        let tokens = Self::tokenize(text);

        // Add new terms to vocabulary
        for token in &tokens {
            if !self.vocabulary.contains_key(token) {
                let idx = self.vocabulary.len();
                self.vocabulary.insert(token.clone(), idx);
                self.idf_scores.push(0.0); // placeholder, will be updated
            }
        }

        // Recompute IDF for all terms based on updated doc count
        let num_docs = (self.doc_vectors.len() + 1) as f64;
        let mut doc_freq = vec![0usize; self.vocabulary.len()];

        // Count document frequency from existing vectors
        for vec in &self.doc_vectors {
            for &idx in &vec.indices {
                if idx < doc_freq.len() {
                    doc_freq[idx] += 1;
                }
            }
        }

        // Count the new document's contribution
        let mut seen: HashMap<&str, bool> = HashMap::new();
        for token in &tokens {
            if !seen.contains_key(token.as_str()) {
                seen.insert(token.as_str(), true);
                if let Some(&idx) = self.vocabulary.get(token) {
                    doc_freq[idx] += 1;
                }
            }
        }

        // Update IDF scores
        self.idf_scores = doc_freq
            .iter()
            .map(|&df| (num_docs / (1.0 + df as f64)).ln())
            .collect();

        // Compute TF-IDF vector for the new document
        let vector = self.compute_tfidf_vector(&tokens);
        self.doc_vectors.push(vector);
    }

    /// Queries the index and returns the top-k most similar documents.
    ///
    /// Returns a vector of `(doc_index, score)` tuples sorted by score descending,
    /// limited to `top_k` results with score > 0.
    pub fn query(&self, text: &str, top_k: usize) -> Vec<(usize, f64)> {
        if self.doc_vectors.is_empty() {
            return Vec::new();
        }

        let tokens = Self::tokenize(text);
        if tokens.is_empty() {
            return Vec::new();
        }

        let query_vector = self.compute_tfidf_vector(&tokens);
        let query_magnitude = Self::magnitude(&query_vector);

        if query_magnitude == 0.0 {
            return Vec::new();
        }

        let mut scores: Vec<(usize, f64)> = self
            .doc_vectors
            .iter()
            .enumerate()
            .filter_map(|(idx, doc_vec)| {
                let doc_magnitude = Self::magnitude(doc_vec);
                if doc_magnitude == 0.0 {
                    return None;
                }
                let dot = Self::dot_product(&query_vector, doc_vec);
                let similarity = dot / (query_magnitude * doc_magnitude);
                if similarity > 0.0 {
                    Some((idx, similarity))
                } else {
                    None
                }
            })
            .collect();

        // Sort by score descending
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Limit to top_k
        scores.truncate(top_k);
        scores
    }

    /// Tokenizes text: lowercase, split on non-alphanumeric, filter stopwords and empty tokens.
    fn tokenize(text: &str) -> Vec<String> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .filter(|s| !STOPWORDS.contains(s))
            .map(|s| s.to_string())
            .collect()
    }

    /// Computes a TF-IDF sparse vector for a list of tokens.
    fn compute_tfidf_vector(&self, tokens: &[String]) -> SparseVector {
        if tokens.is_empty() {
            return SparseVector {
                indices: Vec::new(),
                values: Vec::new(),
            };
        }

        // Compute term frequencies
        let mut term_counts: HashMap<usize, usize> = HashMap::new();
        for token in tokens {
            if let Some(&idx) = self.vocabulary.get(token) {
                *term_counts.entry(idx).or_insert(0) += 1;
            }
        }

        let total_terms = tokens.len() as f64;

        // Build sparse vector with TF-IDF values
        let mut indices: Vec<usize> = term_counts.keys().cloned().collect();
        indices.sort();

        let values: Vec<f64> = indices
            .iter()
            .map(|&idx| {
                let tf = *term_counts.get(&idx).unwrap() as f64 / total_terms;
                let idf = if idx < self.idf_scores.len() {
                    self.idf_scores[idx]
                } else {
                    0.0
                };
                tf * idf
            })
            .collect();

        SparseVector { indices, values }
    }

    /// Computes the dot product of two sparse vectors.
    fn dot_product(a: &SparseVector, b: &SparseVector) -> f64 {
        let mut result = 0.0;
        let mut i = 0;
        let mut j = 0;

        while i < a.indices.len() && j < b.indices.len() {
            match a.indices[i].cmp(&b.indices[j]) {
                std::cmp::Ordering::Equal => {
                    result += a.values[i] * b.values[j];
                    i += 1;
                    j += 1;
                }
                std::cmp::Ordering::Less => {
                    i += 1;
                }
                std::cmp::Ordering::Greater => {
                    j += 1;
                }
            }
        }

        result
    }

    /// Computes the magnitude (L2 norm) of a sparse vector.
    fn magnitude(v: &SparseVector) -> f64 {
        v.values.iter().map(|x| x * x).sum::<f64>().sqrt()
    }
}
