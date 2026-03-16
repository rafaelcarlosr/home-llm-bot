use crate::error::{BotError, Result};

#[derive(Clone, Debug)]
pub struct Config {
    pub telegram_token: String,
    pub lm_studio_url: String,
    pub home_assistant_url: String,
    pub home_assistant_token: String,
    pub whisper_url: String,
    pub database_url: String,
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
        std::env::set_var("WHISPER_URL", "http://localhost:9000");
        std::env::set_var("DATABASE_URL", "sqlite:test.db");

        let config = Config::from_env().unwrap();
        assert_eq!(config.telegram_token, "test_token");
        assert_eq!(config.lm_studio_url, "http://localhost:1234");
    }
}
