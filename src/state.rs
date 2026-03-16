use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

pub async fn init_db(database_url: &str) -> crate::error::Result<SqlitePool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;

    sqlx::query(include_str!("../migrations/001_init.sql"))
        .execute(&pool)
        .await?;

    Ok(pool)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: String,  // "user", "assistant", "function"
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub sender_name: Option<String>,
}

#[derive(Debug)]
pub struct ConversationState {
    pub family_id: i64,
    pub messages: Vec<Message>,
}

impl ConversationState {
    pub fn new(family_id: i64) -> Self {
        Self {
            family_id,
            messages: Vec::new(),
        }
    }

    pub fn add_message(&mut self, role: &str, content: &str, sender_name: Option<String>) {
        self.messages.push(Message {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            sender_name,
        });
    }

    pub fn get_context_window(&self, size: usize) -> Vec<Message> {
        if self.messages.len() <= size {
            self.messages.clone()
        } else {
            self.messages[self.messages.len() - size..].to_vec()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_message_to_history() {
        let mut state = ConversationState::new(1);
        state.add_message("user", "Turn on lights", None);

        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.messages[0].role, "user");
        assert_eq!(state.messages[0].content, "Turn on lights");
    }

    #[tokio::test]
    async fn test_get_context_window() {
        let mut state = ConversationState::new(1);
        for i in 0..25 {
            state.add_message("user", &format!("Message {}", i), None);
        }

        let context = state.get_context_window(10);
        assert_eq!(context.len(), 10);
        // Should return last 10 messages
        assert_eq!(context[0].content, "Message 15");
    }

    #[tokio::test]
    async fn test_init_db() {
        let pool = init_db("sqlite::memory:").await.unwrap();
        let result: (i64,) = sqlx::query_as("SELECT 1").fetch_one(&pool).await.unwrap();
        assert_eq!(result.0, 1);
    }
}
