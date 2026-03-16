use thiserror::Error;

#[derive(Error, Debug)]
pub enum BotError {
    #[error("Telegram error: {0}")]
    Telegram(String),

    #[error("LM Studio error: {0}")]
    LMStudio(String),

    #[error("Home Assistant error: {0}")]
    HomeAssistant(String),

    #[error("Whisper error: {0}")]
    Whisper(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Configuration error: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, BotError>;
