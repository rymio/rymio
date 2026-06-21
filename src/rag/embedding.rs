use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::RagError;

/// Determines how embeddings are generated.
#[derive(Debug, Clone)]
pub enum EmbeddingMode {
    /// Use an OpenAI-compatible embedding API endpoint.
    Api {
        base_url: String,
        api_key: String,
        model: String,
    },
    /// Fall back to TF-IDF keyword similarity (no real embeddings).
    TfIdf,
}

/// Generates vector embeddings for text content.
pub struct EmbeddingGenerator {
    mode: EmbeddingMode,
    client: Client,
}

/// Request body for the embeddings API (OpenAI-compatible format).
#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: EmbeddingInput,
}

/// The input field can be a single string or a batch of strings.
#[derive(Serialize)]
#[serde(untagged)]
enum EmbeddingInput {
    Single(String),
    Batch(Vec<String>),
}

/// Response from the embeddings API (OpenAI-compatible format).
#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

/// A single embedding result within the API response.
#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

impl EmbeddingGenerator {
    /// Create a new EmbeddingGenerator with the given mode.
    /// Builds a reqwest client with a 30-second timeout.
    pub fn new(mode: EmbeddingMode) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self { mode, client }
    }

    /// Generate an embedding for a single text string.
    ///
    /// In TfIdf mode, returns an error since TF-IDF does not produce vector embeddings.
    /// In Api mode, POSTs to `{base_url}/embeddings` and parses the response.
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>, RagError> {
        match &self.mode {
            EmbeddingMode::TfIdf => Err(RagError::EmbeddingError(
                "TF-IDF mode does not generate embeddings".to_string(),
            )),
            EmbeddingMode::Api {
                base_url,
                api_key,
                model,
            } => {
                let url = format!("{}/embeddings", base_url.trim_end_matches('/'));
                let request_body = EmbeddingRequest {
                    model: model.clone(),
                    input: EmbeddingInput::Single(text.to_string()),
                };

                let response = self
                    .client
                    .post(&url)
                    .header("Authorization", format!("Bearer {}", api_key))
                    .header("Content-Type", "application/json")
                    .json(&request_body)
                    .send()
                    .await
                    .map_err(|e| RagError::EmbeddingError(format!("HTTP request failed: {e}")))?;

                if !response.status().is_success() {
                    let status = response.status();
                    let body = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "unable to read response body".to_string());
                    return Err(RagError::EmbeddingError(format!(
                        "API returned status {status}: {body}"
                    )));
                }

                let embedding_response: EmbeddingResponse = response.json().await.map_err(|e| {
                    RagError::EmbeddingError(format!("Failed to parse API response: {e}"))
                })?;

                embedding_response
                    .data
                    .into_iter()
                    .next()
                    .map(|d| d.embedding)
                    .ok_or_else(|| {
                        RagError::EmbeddingError(
                            "API response contained no embedding data".to_string(),
                        )
                    })
            }
        }
    }

    /// Generate embeddings for a batch of texts.
    ///
    /// In TfIdf mode, returns an error.
    /// In Api mode, sends all texts in a single batch request.
    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, RagError> {
        match &self.mode {
            EmbeddingMode::TfIdf => Err(RagError::EmbeddingError(
                "TF-IDF mode does not generate embeddings".to_string(),
            )),
            EmbeddingMode::Api {
                base_url,
                api_key,
                model,
            } => {
                if texts.is_empty() {
                    return Ok(Vec::new());
                }

                let url = format!("{}/embeddings", base_url.trim_end_matches('/'));
                let request_body = EmbeddingRequest {
                    model: model.clone(),
                    input: EmbeddingInput::Batch(texts.to_vec()),
                };

                let response = self
                    .client
                    .post(&url)
                    .header("Authorization", format!("Bearer {}", api_key))
                    .header("Content-Type", "application/json")
                    .json(&request_body)
                    .send()
                    .await
                    .map_err(|e| RagError::EmbeddingError(format!("HTTP request failed: {e}")))?;

                if !response.status().is_success() {
                    let status = response.status();
                    let body = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "unable to read response body".to_string());
                    return Err(RagError::EmbeddingError(format!(
                        "API returned status {status}: {body}"
                    )));
                }

                let embedding_response: EmbeddingResponse = response.json().await.map_err(|e| {
                    RagError::EmbeddingError(format!("Failed to parse API response: {e}"))
                })?;

                Ok(embedding_response
                    .data
                    .into_iter()
                    .map(|d| d.embedding)
                    .collect())
            }
        }
    }

    /// Returns a reference to the current embedding mode.
    pub fn mode(&self) -> &EmbeddingMode {
        &self.mode
    }
}
