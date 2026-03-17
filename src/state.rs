use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;

pub async fn init_db(database_url: &str) -> crate::error::Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(pool)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: String,  // "user", "assistant", "tool"
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub sender_name: Option<String>,
}

/// Shared conversation state for the whole family.
///
/// **Rust learning note on `Option<SqlitePool>`:**
/// `pool` is `Option<SqlitePool>` — either `Some(pool)` if persistence is enabled,
/// or `None` for in-memory-only mode (used in tests).
/// In Java you'd use `@Nullable SqlitePool pool` + null checks.
/// Rust forces you to handle both cases explicitly via `if let Some(pool) = &self.pool`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BotMode {
    /// Normal: voice messages are transcribed and sent to the LLM.
    Respond,
    /// Transcribe-only: voice messages are echoed as text, LLM is not called.
    TranscribeOnly,
}

#[derive(Debug)]
pub struct ConversationState {
    pub family_id: i64,
    pub messages: Vec<Message>,
    pub mode: BotMode,
    pool: Option<SqlitePool>,
}

impl ConversationState {
    /// In-memory only (no persistence). Used in tests.
    pub fn new(family_id: i64) -> Self {
        Self {
            family_id,
            messages: Vec::new(),
            mode: BotMode::Respond,
            pool: None,
        }
    }

    /// With SQLite persistence. Used in production.
    pub fn with_db(family_id: i64, pool: SqlitePool) -> Self {
        Self {
            family_id,
            messages: Vec::new(),
            mode: BotMode::Respond,
            pool: Some(pool),
        }
    }

    /// Add a message in-memory (no DB write). Fast path for context building.
    pub fn add_message(&mut self, role: &str, content: &str, sender_name: Option<String>) {
        self.messages.push(Message {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            sender_name,
        });
    }

    /// Add a message and persist to SQLite if a pool is configured.
    ///
    /// **Rust learning note on `if let`:**
    /// `if let Some(pool) = &self.pool` unpacks the Option.
    /// If `pool` is `None`, the block is skipped entirely.
    /// Java equivalent: `if (this.pool != null) { ... }`
    /// The difference: Rust *won't compile* if you try to use `pool` without this check.
    pub async fn add_message_persisted(
        &mut self,
        role: &str,
        content: &str,
        sender_name: Option<String>,
    ) -> crate::error::Result<()> {
        self.add_message(role, content, sender_name.clone());

        if let Some(pool) = &self.pool {
            // Ensure a conversation row exists for this family_id
            sqlx::query(
                "INSERT OR IGNORE INTO conversations (family_id) VALUES (?)"
            )
            .bind(self.family_id)
            .execute(pool)
            .await?;

            // Get the conversation id
            let conv_id: i64 = sqlx::query_scalar(
                "SELECT id FROM conversations WHERE family_id = ? ORDER BY id DESC LIMIT 1"
            )
            .bind(self.family_id)
            .fetch_one(pool)
            .await?;

            // Persist the message
            sqlx::query(
                "INSERT INTO messages (conversation_id, role, content, sender_name) VALUES (?, ?, ?, ?)"
            )
            .bind(conv_id)
            .bind(role)
            .bind(content)
            .bind(sender_name.as_deref())
            .execute(pool)
            .await?;
        }

        Ok(())
    }

    /// Load conversation history from SQLite for a given family.
    ///
    /// **Rust learning note on `query_as`:**
    /// `sqlx::query_as::<_, (String, String, Option<String>)>` maps each row
    /// to a typed tuple at compile time. Similar to Spring's `RowMapper<T>`.
    /// The `Option<String>` for sender_name maps a nullable column — the compiler
    /// forces you to handle the None case.
    pub async fn load_history(
        pool: &SqlitePool,
        family_id: i64,
        limit: i64,
    ) -> crate::error::Result<Vec<Message>> {
        let rows = sqlx::query_as::<_, (String, String, Option<String>, String)>(
            "SELECT role, content, sender_name, timestamp FROM (
               SELECT m.role, m.content, m.sender_name, m.timestamp, m.id
               FROM messages m
               JOIN conversations c ON m.conversation_id = c.id
               WHERE c.family_id = ?
               ORDER BY m.id DESC
               LIMIT ?
             ) ORDER BY id ASC",
        )
        .bind(family_id)
        .bind(limit)
        .fetch_all(pool)
        .await?;

        let messages = rows
            .into_iter()
            .map(|(role, content, sender_name, ts)| Message {
                role,
                content,
                sender_name,
                timestamp: ts
                    .parse::<DateTime<Utc>>()
                    .unwrap_or_else(|_| Utc::now()),
            })
            .collect();

        Ok(messages)
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
        assert_eq!(context[0].content, "Message 15");
    }

    #[tokio::test]
    async fn test_init_db() {
        let pool = init_db("sqlite::memory:").await.unwrap();
        let result: (i64,) = sqlx::query_as("SELECT 1").fetch_one(&pool).await.unwrap();
        assert_eq!(result.0, 1);
    }

    /// Regression: SQLx 0.7 defaults create_if_missing=false; file DBs would fail without explicit flag.
    #[tokio::test]
    async fn test_init_db_creates_file_if_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let url = format!("sqlite://{}", path.display());
        init_db(&url).await.unwrap();
        assert!(path.exists());
    }

    /// Regression: calling init_db twice on the same DB must not fail (migration idempotency).
    #[tokio::test]
    async fn test_init_db_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let url = format!("sqlite://{}", path.display());
        init_db(&url).await.unwrap();
        init_db(&url).await.unwrap(); // must not error
    }

    #[tokio::test]
    async fn test_persist_and_load_message() {
        let pool = init_db("sqlite::memory:").await.unwrap();
        let mut state = ConversationState::with_db(1, pool.clone());

        state.add_message_persisted("user", "hello", None).await.unwrap();
        state.add_message_persisted("assistant", "hi there", None).await.unwrap();

        let history = ConversationState::load_history(&pool, 1, 10).await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].role, "user");
        assert_eq!(history[0].content, "hello");
        assert_eq!(history[1].role, "assistant");
        assert_eq!(history[1].content, "hi there");
    }
}
