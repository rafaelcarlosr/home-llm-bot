use std::sync::Arc;
use teloxide::net::Download;
use teloxide::prelude::*;
use teloxide::types::{FileId, Message};
use tokio::sync::Mutex;
use crate::orchestrator::Orchestrator;
use crate::plugins::whisper::WhisperProvider;
use crate::state::ConversationState;
use crate::error::Result;

/// Start the Telegram bot dispatcher.
///
/// **Rust learning note on `Arc<T>`:**
/// `Arc` = Atomic Reference Counted pointer. Multiple tasks can hold a clone
/// of an `Arc` and they all point to the same data on the heap.
/// `Arc::clone(&x)` increments the reference count (O(1), cheap).
/// When all clones are dropped, the data is freed.
/// Java: all object references are essentially `Arc` managed by the GC.
/// In Rust, you choose when to use shared ownership explicitly.
///
/// **Rust learning note on `tokio::sync::Mutex` vs `std::sync::Mutex`:**
/// Use `tokio::sync::Mutex` when the lock must be held across `.await` points.
/// `std::sync::Mutex` is not `Send` across await points — the compiler rejects it.
/// Java's `synchronized` has no such distinction since threads block synchronously.
pub async fn start(
    token: String,
    orchestrator: Arc<Orchestrator>,
    whisper: Arc<WhisperProvider>,
    state: Arc<Mutex<ConversationState>>,
) -> Result<()> {
    let bot = Bot::new(&token);
    tracing::info!("Telegram bot started, listening for messages...");

    // Clone Arcs for the closure — cheap (just increments ref count)
    let orch = Arc::clone(&orchestrator);
    let wh = Arc::clone(&whisper);
    let st = Arc::clone(&state);

    // `teloxide::repl` runs a simple message loop.
    // Each message spawns a new async task with cloned Arc references.
    // **Rust learning note on `move` closures:**
    // The `move` keyword transfers ownership of `orch`, `wh`, `st` into the closure.
    // Inside, we clone the Arcs again per-message so each async task has its own clone.
    // In Java, lambdas capture effectively-final references; Rust makes ownership explicit.
    teloxide::repl(bot, move |bot: Bot, msg: Message| {
        let orch = Arc::clone(&orch);
        let wh = Arc::clone(&wh);
        let st = Arc::clone(&st);

        async move {
            if let Err(e) = handle_message(bot, msg, orch, wh, st).await {
                tracing::error!("Error handling message: {}", e);
            }
            respond(())
        }
    })
    .await;

    Ok(())
}

/// Route incoming messages to the appropriate handler.
async fn handle_message(
    bot: Bot,
    msg: Message,
    orch: Arc<Orchestrator>,
    whisper: Arc<WhisperProvider>,
    state: Arc<Mutex<ConversationState>>,
) -> anyhow::Result<()> {
    let chat_id = msg.chat.id;
    let sender = msg
        .from
        .as_ref()
        .and_then(|u| u.username.as_deref())
        .unwrap_or(&msg.from.as_ref().map(|u| u.first_name.as_str()).unwrap_or("unknown"))
        .to_string();

    if let Some(text) = msg.text() {
        handle_text(&bot, chat_id, text, &sender, &orch, &state).await?;
    } else if let Some(voice) = msg.voice() {
        handle_voice(&bot, chat_id, voice.file.id.clone(), &sender, &orch, &whisper, &state).await?;
    }

    Ok(())
}

/// Handle a text message: send to orchestrator and reply.
///
/// **Rust learning note on lock scope:**
/// The lock is acquired and released inside the inner block `{ ... }`.
/// This ensures the Mutex is unlocked *before* the `.await` on `send_message`.
/// Holding a lock across an `.await` point would block other tasks from accessing
/// the state while waiting for Telegram's HTTP response — a common async pitfall.
/// In Java, you'd use a synchronized block carefully around only the critical section.
async fn handle_text(
    bot: &Bot,
    chat_id: ChatId,
    text: &str,
    sender: &str,
    orch: &Arc<Orchestrator>,
    state: &Arc<Mutex<ConversationState>>,
) -> anyhow::Result<()> {
    tracing::info!("Text from {}: {}", sender, text);

    // Lock, process, unlock — all before the HTTP send
    let response = {
        let mut guard = state.lock().await;
        orch.process_message(text, &mut guard).await
    }; // <- MutexGuard drops here, lock released

    match response {
        Ok(reply) => {
            bot.send_message(chat_id, reply).await?;
        }
        Err(e) => {
            tracing::error!("Orchestrator error: {}", e);
            bot.send_message(chat_id, "Sorry, something went wrong.").await?;
        }
    }

    Ok(())
}

/// Handle a voice message: transcribe with Whisper, then process as text.
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

    // Step 1: Get file info from Telegram
    let file = bot.get_file(file_id).await?;

    // Step 2: Download audio bytes
    let mut audio_bytes: Vec<u8> = Vec::new();
    bot.download_file(&file.path, &mut audio_bytes).await?;

    // Step 3: Transcribe with Whisper
    let transcription = match whisper.transcribe(audio_bytes).await {
        Ok(text) => text,
        Err(e) => {
            tracing::error!("Whisper error: {}", e);
            bot.send_message(chat_id, "Could not transcribe voice message.").await?;
            return Ok(());
        }
    };

    tracing::info!("Transcribed: {}", transcription);

    // Show the user what was heard
    bot.send_message(chat_id, format!("🎤 Heard: *{}*", transcription))
        .parse_mode(teloxide::types::ParseMode::MarkdownV2)
        .await?;

    // Step 4: Process the transcription as text
    handle_text(bot, chat_id, &transcription, sender, orch, state).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[test]
    fn test_arc_mutex_construction() {
        // Verify that Arc<Mutex<ConversationState>> can be constructed
        // This is a compile-time check that the type wiring is correct
        let state = ConversationState::new(1);
        let _shared: Arc<Mutex<ConversationState>> = Arc::new(Mutex::new(state));
    }
}
