use teloxide::prelude::*;
use crate::error::Result;

pub struct TelegramHandler {
    token: String,
}

impl TelegramHandler {
    pub fn new(token: String) -> Self {
        Self { token }
    }

    pub async fn start(&self) -> Result<()> {
        let _bot = Bot::new(&self.token);
        tracing::info!("Starting Telegram bot");

        // TODO: Set up handlers for messages

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_handle_text_message() {
        let handler = TelegramHandler::new("test_token".to_string());
        assert_eq!(handler.token, "test_token");
    }
}
