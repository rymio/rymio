// LLMClient, async HTTP communication

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::prompts::Message;

/// Configuration for the LLM client.
#[derive(Debug, Clone)]
pub struct LLMConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
}

/// Response from the LLM API.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct LLMResponse {
    pub content: String,
    pub finish_reason: String,
    pub usage: Option<Usage>,
}

/// Token usage information from the LLM API response.
#[derive(Deserialize, Clone, Debug)]
#[allow(dead_code)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Errors that can occur during LLM communication.
#[derive(Debug)]
pub enum LLMError {
    /// Server is unreachable.
    ConnectionError(String),
    /// Request timed out.
    TimeoutError(String),
    /// Non-2xx HTTP response.
    HttpError { status: u16, message: String },
    /// Failed to parse response JSON.
    ParseError(String),
}

impl std::fmt::Display for LLMError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConnectionError(msg) => write!(f, "Connection error: {msg}"),
            Self::TimeoutError(msg) => write!(f, "Timeout: {msg}"),
            Self::HttpError { status, message } => write!(f, "HTTP {status}: {message}"),
            Self::ParseError(msg) => write!(f, "Parse error: {msg}"),
        }
    }
}

impl std::error::Error for LLMError {}

// Internal request/response types for serde

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessagePayload>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Serialize)]
struct ChatMessagePayload {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChoiceMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    content: Option<String>,
}

/// Async HTTP client for communicating with OpenAI-compatible LLM APIs.
pub struct LLMClient {
    client: reqwest::Client,
    config: LLMConfig,
}

impl LLMClient {
    /// Create a new LLMClient with the given configuration.
    ///
    /// Builds an HTTP client with a 5-second connect timeout and 60-second
    /// overall request timeout.
    pub fn new(config: LLMConfig) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to build HTTP client");

        Self { client, config }
    }

    /// Send a chat completion request to the LLM API.
    ///
    /// Posts the given messages to {base_url}/chat/completions with the
    /// configured model, temperature, and max_tokens. Returns the assistant's
    /// response content, finish reason, and token usage.
    pub async fn chat(&self, messages: Vec<Message>) -> Result<LLMResponse, LLMError> {
        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );

        let payload = ChatRequest {
            model: self.config.model.clone(),
            messages: messages
                .into_iter()
                .map(|m| ChatMessagePayload {
                    role: m.role,
                    content: m.content,
                })
                .collect(),
            temperature: self.config.temperature,
            max_tokens: self.config.max_tokens,
        };

        let mut request = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");

        if !self.config.api_key.is_empty() {
            request = request.header("Authorization", format!("Bearer {}", self.config.api_key));
        }

        let response = request.json(&payload).send().await.map_err(|e| {
            if e.is_timeout() {
                LLMError::TimeoutError(format!("Request timed out: {e}"))
            } else if e.is_connect() {
                LLMError::ConnectionError(format!("Cannot connect to LLM server: {e}"))
            } else {
                LLMError::ConnectionError(format!("Request failed: {e}"))
            }
        })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(LLMError::HttpError {
                status: status.as_u16(),
                message: body,
            });
        }

        let body: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| LLMError::ParseError(format!("Failed to parse response: {e}")))?;

        let choice = body
            .choices
            .first()
            .ok_or_else(|| LLMError::ParseError("No choices in response".to_string()))?;

        Ok(LLMResponse {
            content: choice.message.content.clone().unwrap_or_default(),
            finish_reason: choice
                .finish_reason
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            usage: body.usage,
        })
    }

    /// Truncate content to the specified maximum character count.
    ///
    /// Returns the (possibly truncated) string and a flag indicating whether
    /// truncation occurred.
    #[allow(dead_code)]
    pub fn truncate_content(&self, content: &str, max_chars: usize) -> (String, bool) {
        if content.len() <= max_chars {
            (content.to_string(), false)
        } else {
            (content[..max_chars].to_string(), true)
        }
    }
}
