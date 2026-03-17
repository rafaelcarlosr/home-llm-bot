use dotenv::dotenv;
use home_llm_bot::{
    config::Config,
    orchestrator::Orchestrator,
    plugins::{
        PluginRegistry,
        home_assistant::HomeAssistantPlugin,
        lm_studio::LMStudioProvider,
        whisper::WhisperProvider,
    },
    state::init_db,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Entry point for home-llm-bot.
///
/// **Rust learning note on `Box<dyn std::error::Error>`:**
/// The `?` operator in main requires errors to implement `std::error::Error`.
/// `Box<dyn Error>` is a trait object that accepts any error type — equivalent
/// to Java's `main() throws Exception`. Using `?` propagates any error and
/// prints it on exit.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env file if present (ignored if missing — env vars may be set externally)
    // **Rust learning note:** `.ok()` converts Result to Option, discarding the error.
    // Equivalent to: try { dotenv(); } catch(Exception e) { /* ignore */ }
    dotenv().ok();

    tracing_subscriber::fmt::init();
    tracing::info!("Starting home-llm-bot");

    // Load all configuration from environment variables
    let config = Config::from_env()?;
    tracing::info!("Configuration loaded");

    // Initialize SQLite database and run migrations
    let pool = init_db(&config.database_url).await?;
    tracing::info!("Database initialized");

    // Build plugin registry with all available integrations
    // **Rust learning note on `Box<dyn Plugin>`:**
    // `Box<dyn Plugin>` is a heap-allocated trait object — similar to Java's
    // interface reference `Plugin plugin = new HomeAssistantPlugin(...)`.
    // `Box` gives us heap allocation; `dyn` means dynamic dispatch at runtime.
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(HomeAssistantPlugin::new(
        config.home_assistant_url.clone(),
        config.home_assistant_token.clone(),
    )));
    tracing::info!("Registered {} functions", registry.get_all_functions().len());

    // LLM model name — defaults to a sensible local model if not set
    let model = std::env::var("LLM_MODEL")
        .unwrap_or_else(|_| "qwen2.5-7b-instruct".to_string());

    // Build the orchestrator (owns the LLM provider and plugin registry)
    let lm_provider = LMStudioProvider::new(config.lm_studio_url.clone());
    let orchestrator = Arc::new(Orchestrator::new(lm_provider, registry, model));

    // Build the Whisper STT provider
    let whisper = Arc::new(WhisperProvider::new(config.whisper_url.clone()));

    // Per-chat state: lazily initialised on first message from each chat.
    // Each chat gets its own conversation history, mode, and context window.
    // **Rust learning note:** `Arc<Mutex<HashMap<...>>>` — outer lock is held
    // briefly just to look up the inner `Arc<Mutex<ConversationState>>` for a
    // chat; the inner lock is then held during message processing. This lets
    // different chats process messages concurrently.
    let states = Arc::new(Mutex::new(HashMap::new()));

    // Start the Telegram bot (blocks until shutdown)
    tracing::info!("Starting Telegram bot...");
    home_llm_bot::telegram::start(
        config.telegram_token,
        orchestrator,
        whisper,
        states,
        pool,
    )
    .await?;

    Ok(())
}
