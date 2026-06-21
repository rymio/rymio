use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct ProviderPreset {
    pub base_url: &'static str,
    pub model: &'static str,
}

pub static PROVIDER_PRESETS: Lazy<HashMap<&'static str, ProviderPreset>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert(
        "ollama",
        ProviderPreset {
            base_url: "http://127.0.0.1:11434/v1",
            model: "qwen2.5-coder:7b",
        },
    );
    m.insert(
        "llama.cpp",
        ProviderPreset {
            base_url: "http://127.0.0.1:8080/v1",
            model: "local",
        },
    );
    m.insert(
        "openai",
        ProviderPreset {
            base_url: "https://api.openai.com/v1",
            model: "gpt-4o-mini",
        },
    );
    m.insert(
        "deepseek",
        ProviderPreset {
            base_url: "https://api.deepseek.com/v1",
            model: "deepseek-chat",
        },
    );
    m.insert(
        "groq",
        ProviderPreset {
            base_url: "https://api.groq.com/openai/v1",
            model: "llama-3.1-70b-versatile",
        },
    );
    m.insert(
        "together",
        ProviderPreset {
            base_url: "https://api.together.xyz/v1",
            model: "meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo",
        },
    );
    m.insert(
        "openrouter",
        ProviderPreset {
            base_url: "https://openrouter.ai/api/v1",
            model: "meta-llama/llama-3.1-70b-instruct",
        },
    );
    m
});

pub fn is_local_provider(provider: &str) -> bool {
    provider == "ollama" || provider == "llama.cpp"
}

pub fn default_api_key_for_provider(provider: &str) -> &'static str {
    if is_local_provider(provider) {
        "local"
    } else {
        ""
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub max_file_kb: u32,
    pub max_prompt_chars: usize,
    pub temperature: f32,
    pub max_tokens: u32,
    pub ignored_directories: Vec<String>,
    pub secret_patterns: Vec<String>,
    pub test_command: String,
    pub rag_enabled: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".to_string(),
            base_url: String::new(),
            api_key: String::new(),
            model: String::new(),
            max_file_kb: 300,
            max_prompt_chars: 30000,
            temperature: 0.2,
            max_tokens: 4096,
            ignored_directories: vec![
                ".git",
                ".venv",
                "venv",
                "env",
                "node_modules",
                "__pycache__",
                "dist",
                "build",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            secret_patterns: vec![".env", "id_rsa", "*.pem", "*.key", "settings_local.py"]
                .into_iter()
                .map(String::from)
                .collect(),
            test_command: "python -m pytest".to_string(),
            rag_enabled: false,
        }
    }
}

/// Returns the home directory path, or None if it cannot be determined.
fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

/// Load application configuration from disk.
///
/// Search order:
/// 1. `{root}/config.json`
/// 2. `~/.config/litecode-agent/config.json`
///
/// If a file is found but contains invalid JSON, a warning is printed to stderr
/// and the default configuration is returned. If no file is found, the default
/// configuration is returned.
///
/// After loading, provider preset defaults are applied for empty `base_url` and
/// `model` fields. For local providers (ollama, llama.cpp), `api_key` defaults
/// to "local" when not set.
pub fn load_config(root: &Path) -> AppConfig {
    let config_paths: Vec<PathBuf> = {
        let mut paths = vec![root.join("config.json")];
        if let Some(home) = home_dir() {
            paths.push(home.join(".config/litecode-agent/config.json"));
        }
        paths
    };

    let mut config = AppConfig::default();

    for path in &config_paths {
        if path.exists() {
            match fs::read_to_string(path) {
                Ok(content) => match serde_json::from_str::<AppConfig>(&content) {
                    Ok(loaded) => {
                        config = loaded;
                        break;
                    }
                    Err(e) => {
                        eprintln!("Warning: Invalid JSON in {}: {}", path.display(), e);
                        return AppConfig::default();
                    }
                },
                Err(e) => {
                    eprintln!("Warning: Cannot read {}: {}", path.display(), e);
                }
            }
        }
    }

    // Apply provider preset defaults
    if let Some(preset) = PROVIDER_PRESETS.get(config.provider.as_str()) {
        if config.base_url.is_empty() {
            config.base_url = preset.base_url.to_string();
        }
        if config.model.is_empty() {
            config.model = preset.model.to_string();
        }
    }

    // Default api_key for local providers
    if config.api_key.is_empty() {
        config.api_key = default_api_key_for_provider(&config.provider).to_string();
    }

    config
}

/// Save the application configuration to `{root}/config.json`.
///
/// Writes the config as pretty-printed JSON. If the file cannot be written,
/// returns an error message string.
pub fn save_config(root: &Path, config: &AppConfig) -> Result<(), String> {
    let config_path = root.join("config.json");
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {e}"))?;
    fs::write(&config_path, json)
        .map_err(|e| format!("Failed to write config to {}: {e}", config_path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_load_config_no_file_returns_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let config = load_config(tmp.path());

        assert_eq!(config.provider, "ollama");
        assert_eq!(config.base_url, "http://127.0.0.1:11434/v1");
        assert_eq!(config.model, "qwen2.5-coder:7b");
        assert_eq!(config.api_key, "local");
        assert_eq!(config.max_file_kb, 300);
        assert_eq!(config.max_prompt_chars, 30000);
        assert!((config.temperature - 0.2).abs() < f32::EPSILON);
        assert_eq!(config.max_tokens, 4096);
    }

    #[test]
    fn test_load_config_valid_json_in_project_root() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.json");
        fs::write(
            &config_path,
            r#"{"provider": "openai", "api_key": "sk-test123"}"#,
        )
        .unwrap();

        let config = load_config(tmp.path());

        assert_eq!(config.provider, "openai");
        assert_eq!(config.api_key, "sk-test123");
        assert_eq!(config.base_url, "https://api.openai.com/v1");
        assert_eq!(config.model, "gpt-4o-mini");
    }

    #[test]
    fn test_load_config_invalid_json_returns_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.json");
        fs::write(&config_path, "not valid json {{{").unwrap();

        let config = load_config(tmp.path());

        // Should fall back to defaults with preset applied
        assert_eq!(config.provider, "ollama");
        assert_eq!(config.base_url, "");
        assert_eq!(config.model, "");
        assert_eq!(config.api_key, "");
    }

    #[test]
    fn test_load_config_partial_json_uses_defaults_for_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.json");
        fs::write(&config_path, r#"{"provider": "deepseek"}"#).unwrap();

        let config = load_config(tmp.path());

        assert_eq!(config.provider, "deepseek");
        assert_eq!(config.base_url, "https://api.deepseek.com/v1");
        assert_eq!(config.model, "deepseek-chat");
        // Non-local provider, api_key stays empty
        assert_eq!(config.api_key, "");
        // Other fields should be defaults
        assert_eq!(config.max_file_kb, 300);
    }

    #[test]
    fn test_load_config_ollama_defaults_api_key_to_local() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.json");
        fs::write(&config_path, r#"{"provider": "ollama"}"#).unwrap();

        let config = load_config(tmp.path());

        assert_eq!(config.provider, "ollama");
        assert_eq!(config.api_key, "local");
    }

    #[test]
    fn test_load_config_llama_cpp_defaults_api_key_to_local() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.json");
        fs::write(&config_path, r#"{"provider": "llama.cpp"}"#).unwrap();

        let config = load_config(tmp.path());

        assert_eq!(config.provider, "llama.cpp");
        assert_eq!(config.api_key, "local");
        assert_eq!(config.base_url, "http://127.0.0.1:8080/v1");
        assert_eq!(config.model, "local");
    }

    #[test]
    fn test_load_config_explicit_base_url_not_overridden() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.json");
        fs::write(
            &config_path,
            r#"{"provider": "openai", "base_url": "http://custom.endpoint/v1"}"#,
        )
        .unwrap();

        let config = load_config(tmp.path());

        assert_eq!(config.base_url, "http://custom.endpoint/v1");
        // model should still get preset default since it was empty
        assert_eq!(config.model, "gpt-4o-mini");
    }

    #[test]
    fn test_load_config_explicit_api_key_not_overridden_for_local() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.json");
        fs::write(
            &config_path,
            r#"{"provider": "ollama", "api_key": "my-custom-key"}"#,
        )
        .unwrap();

        let config = load_config(tmp.path());

        assert_eq!(config.api_key, "my-custom-key");
    }

    #[test]
    fn test_load_config_unknown_provider_no_preset() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.json");
        fs::write(
            &config_path,
            r#"{"provider": "custom-provider", "base_url": "http://example.com/v1", "model": "my-model"}"#,
        )
        .unwrap();

        let config = load_config(tmp.path());

        assert_eq!(config.provider, "custom-provider");
        assert_eq!(config.base_url, "http://example.com/v1");
        assert_eq!(config.model, "my-model");
        // Not a local provider, api_key stays empty
        assert_eq!(config.api_key, "");
    }
}
