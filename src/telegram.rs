use std::collections::HashMap;
use std::sync::Arc;
use teloxide::net::Download;
use teloxide::prelude::*;
use teloxide::types::{FileId, Message};
use tokio::sync::Mutex;
use sqlx::sqlite::SqlitePool;
use crate::orchestrator::Orchestrator;
use crate::plugins::whisper::WhisperProvider;
use crate::state::{BotMode, ConversationState};
use crate::error::Result;

/// One `Arc<Mutex<ConversationState>>` per chat ID.
/// Outer lock: briefly held to look up/insert a chat's state Arc.
/// Inner lock: held during message processing (only blocks that one chat).
type ChatStates = Arc<Mutex<HashMap<i64, Arc<Mutex<ConversationState>>>>>;

/// Get the state for `chat_id`, lazily initialising it with history from DB on first access.
async fn get_or_init(states: &ChatStates, chat_id: i64, pool: &SqlitePool) -> Arc<Mutex<ConversationState>> {
    // Fast path: already exists
    if let Some(st) = states.lock().await.get(&chat_id) {
        return Arc::clone(st);
    }

    // Slow path: load history (no lock held during async DB call)
    let history = ConversationState::load_history(pool, chat_id, 50).await.unwrap_or_default();
    let mut state = ConversationState::with_db(chat_id, pool.clone());
    state.messages = history;
    let entry = Arc::new(Mutex::new(state));

    // Another task may have raced us — use or_insert so we keep one canonical Arc
    let mut map = states.lock().await;
    Arc::clone(map.entry(chat_id).or_insert(entry))
}

pub async fn start(
    token: String,
    orchestrator: Arc<Orchestrator>,
    whisper: Arc<WhisperProvider>,
    states: ChatStates,
    pool: SqlitePool,
) -> Result<()> {
    let bot = Bot::new(&token);
    tracing::info!("Telegram bot started, listening for messages...");

    let orch = Arc::clone(&orchestrator);
    let wh = Arc::clone(&whisper);

    teloxide::repl(bot, move |bot: Bot, msg: Message| {
        let orch = Arc::clone(&orch);
        let wh = Arc::clone(&wh);
        let states = Arc::clone(&states);
        let pool = pool.clone();

        async move {
            if let Err(e) = handle_message(bot, msg, orch, wh, states, pool).await {
                tracing::error!("Error handling message: {}", e);
            }
            respond(())
        }
    })
    .await;

    Ok(())
}

async fn handle_message(
    bot: Bot,
    msg: Message,
    orch: Arc<Orchestrator>,
    whisper: Arc<WhisperProvider>,
    states: ChatStates,
    pool: SqlitePool,
) -> anyhow::Result<()> {
    let chat_id = msg.chat.id;
    let sender = msg
        .from
        .as_ref()
        .and_then(|u| u.username.as_deref())
        .unwrap_or(&msg.from.as_ref().map(|u| u.first_name.as_str()).unwrap_or("unknown"))
        .to_string();

    let state = get_or_init(&states, chat_id.0, &pool).await;

    if let Some(text) = msg.text() {
        match text {
            "/transcribe" => {
                state.lock().await.mode = BotMode::TranscribeOnly;
                bot.send_message(chat_id, "Transcribe-only mode on. Voice messages will be echoed as text without acting on them. Send /respond to switch back.").await?;
            }
            "/respond" => {
                state.lock().await.mode = BotMode::Respond;
                bot.send_message(chat_id, "Respond mode on. Voice messages will be transcribed and acted on.").await?;
            }
            _ => {
                if state.lock().await.mode == BotMode::Respond {
                    handle_text(&bot, chat_id, text, &sender, &orch, &state).await?;
                }
            }
        }
    } else if let Some(voice) = msg.voice() {
        handle_voice(&bot, chat_id, voice.file.id.clone(), &sender, &orch, &whisper, &state).await?;
    } else if let Some(audio) = msg.audio() {
        // Audio files sent as music (e.g. m4a, mp3)
        handle_voice(&bot, chat_id, audio.file.id.clone(), &sender, &orch, &whisper, &state).await?;
    } else if let Some(doc) = msg.document() {
        // File attachments — only transcribe if the MIME type is audio/*
        let is_audio = doc.mime_type.as_ref()
            .map(|m| m.type_().as_str() == "audio")
            .unwrap_or(false);
        if is_audio {
            handle_voice(&bot, chat_id, doc.file.id.clone(), &sender, &orch, &whisper, &state).await?;
        }
    }

    Ok(())
}

/// **Rust learning note on lock scope:**
/// The lock is acquired and released inside the inner block `{ ... }`.
/// This ensures the Mutex is unlocked *before* the `.await` on `send_message`.
/// Holding a lock across an `.await` point would block other tasks from accessing
/// the state while waiting for Telegram's HTTP response — a common async pitfall.
async fn handle_text(
    bot: &Bot,
    chat_id: ChatId,
    text: &str,
    sender: &str,
    orch: &Arc<Orchestrator>,
    state: &Arc<Mutex<ConversationState>>,
) -> anyhow::Result<()> {
    tracing::info!("Text from {}: {}", sender, text);

    let response = {
        let mut guard = state.lock().await;
        orch.process_message(text, Some(sender), &mut guard).await
    }; // <- MutexGuard drops here, lock released

    match response {
        Ok(reply) => send_long_message(bot, chat_id, &reply).await?,
        Err(e) => {
            tracing::error!("Orchestrator error: {}", e);
            bot.send_message(chat_id, "Sorry, something went wrong.").await?;
        }
    }

    Ok(())
}

async fn handle_voice(
    bot: &Bot,
    chat_id: ChatId,
    file_id: FileId,
    sender: &str,
    orch: &Arc<Orchestrator>,
    whisper: &Arc<WhisperProvider>,
    state: &Arc<Mutex<ConversationState>>,
) -> anyhow::Result<()> {
    tracing::info!("Voice message from {}", sender);

    let file = bot.get_file(file_id).await?;
    let mut audio_bytes: Vec<u8> = Vec::new();
    bot.download_file(&file.path, &mut audio_bytes).await?;

    let transcription = match whisper.transcribe(audio_bytes).await {
        Ok(text) => text,
        Err(e) => {
            tracing::error!("Whisper error: {}", e);
            bot.send_message(chat_id, "Could not transcribe voice message.").await?;
            return Ok(());
        }
    };

    tracing::info!("Transcribed: {}", transcription);
    bot.send_message(chat_id, format!("🎤 {}: {}", sender, transcription)).await?;

    let mode = state.lock().await.mode;
    if mode == BotMode::Respond {
        handle_text(bot, chat_id, &transcription, sender, orch, state).await?;
    }

    Ok(())
}

const TELEGRAM_MAX_LEN: usize = 4096;

async fn send_long_message(bot: &Bot, chat_id: ChatId, text: &str) -> anyhow::Result<()> {
    if text.len() <= TELEGRAM_MAX_LEN {
        bot.send_message(chat_id, text).await?;
        return Ok(());
    }

    let mut start = 0;
    while start < text.len() {
        let end = if start + TELEGRAM_MAX_LEN >= text.len() {
            text.len()
        } else {
            let boundary = start + TELEGRAM_MAX_LEN;
            text[start..boundary]
                .rfind(|c: char| c.is_whitespace())
                .map(|i| start + i)
                .unwrap_or(boundary)
        };

        bot.send_message(chat_id, &text[start..end]).await?;
        start = end + 1;
        while start < text.len() && text.as_bytes()[start] == b' ' {
            start += 1;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_per_chat_state_map_construction() {
        // Verifies that the ChatStates type compiles and can hold per-chat state
        let states: ChatStates = Arc::new(Mutex::new(HashMap::new()));
        let state = ConversationState::new(1);
        let chat_arc: Arc<Mutex<ConversationState>> = Arc::new(Mutex::new(state));
        // Simulate inserting a chat state — just checks the types are correct
        let _ = (states, chat_arc);
    }
}
