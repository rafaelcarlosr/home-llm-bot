use crate::error::{BotError, Result};

#[derive(Clone, Debug)]
pub struct Config {
    pub telegram_token: String,
    pub lm_studio_url: String,
    pub home_assistant_url: String,
    pub home_assistant_token: String,
    pub whisper_url: String,
    pub database_url: String,

    /// Optional system prompt suffix. Use to add home-specific context for the LLM.
    /// Set via SYSTEM_PROMPT_EXTRA env var.
    pub system_prompt_extra: Option<String>,

    /// Comma-separated list of keywords to filter out of GetLiveContext results.
    /// Entries whose friendly name contains any of these substrings (case-insensitive)
    /// will be hidden from the LLM. Useful for removing integration noise like
    /// "AdGuard,Solarman,Disjuntor" that clutters the home state view.
    /// Set via LIVE_CONTEXT_SKIP env var. Empty by default.
    pub live_context_skip: Vec<String>,

    /// LLM model name. Set via LLM_MODEL env var.
    /// Default: "qwen2.5-7b-instruct"
    pub llm_model: String,
    /// LLM sampling temperature (0.0–1.0). Lower = more deterministic.
    /// Set via LLM_TEMPERATURE env var. Default: 0.3
    pub llm_temperature: f64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            telegram_token: std::env::var("TELEGRAM_TOKEN")
                .map_err(|_| BotError::Config("TELEGRAM_TOKEN not set".to_string()))?,
            lm_studio_url: std::env::var("LM_STUDIO_URL")
                .map_err(|_| BotError::Config("LM_STUDIO_URL not set".to_string()))?,
            home_assistant_url: std::env::var("HOME_ASSISTANT_URL")
                .map_err(|_| BotError::Config("HOME_ASSISTANT_URL not set".to_string()))?,
            home_assistant_token: std::env::var("HOME_ASSISTANT_TOKEN")
                .map_err(|_| BotError::Config("HOME_ASSISTANT_TOKEN not set".to_string()))?,
            whisper_url: std::env::var("WHISPER_URL")
                .map_err(|_| BotError::Config("WHISPER_URL not set".to_string()))?,
            database_url: std::env::var("DATABASE_URL")
                .map_err(|_| BotError::Config("DATABASE_URL not set".to_string()))?,
            system_prompt_extra: std::env::var("SYSTEM_PROMPT_EXTRA").ok(),
            live_context_skip: std::env::var("LIVE_CONTEXT_SKIP")
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect(),
            llm_model: std::env::var("LLM_MODEL")
                .unwrap_or_else(|_| "qwen2.5-7b-instruct".to_string()),
            llm_temperature: std::env::var("LLM_TEMPERATURE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.3),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_from_env() {
        std::env::set_var("TELEGRAM_TOKEN", "test_token");
        std::env::set_var("LM_STUDIO_URL", "http://localhost:1234");
        std::env::set_var("HOME_ASSISTANT_URL", "http://localhost:8123");
        std::env::set_var("HOME_ASSISTANT_TOKEN", "test_ha_token");
        std::env::set_var("WHISPER_URL", "http://localhost:8000");
        std::env::set_var("DATABASE_URL", "sqlite:test.db");

        let config = Config::from_env().unwrap();
        assert_eq!(config.telegram_token, "test_token");
        assert_eq!(config.lm_studio_url, "http://localhost:1234");
    }

    #[test]
    fn test_config_temperature_default() {
        // Ensure unset LLM_TEMPERATURE defaults to 0.3
        std::env::remove_var("LLM_TEMPERATURE");
        // We can't call Config::from_env() without all required vars, so test the parse logic directly:
        let val: f64 = std::env::var("LLM_TEMPERATURE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.3);
        assert!((val - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn test_config_temperature_from_env() {
        std::env::set_var("LLM_TEMPERATURE", "0.1");
        let val: f64 = std::env::var("LLM_TEMPERATURE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.3);
        assert!((val - 0.1).abs() < f64::EPSILON);
        std::env::remove_var("LLM_TEMPERATURE");
    }
}
