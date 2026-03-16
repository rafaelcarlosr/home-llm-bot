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
    state::{ConversationState, init_db},
};
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

    // Load existing conversation history for the family (family_id = 1)
    // **Rust learning note on `Arc<Mutex<T>>`:**
    // `Arc` = Atomic Reference Count (thread-safe shared ownership, like Java's GC refs).
    // `Mutex` = mutual exclusion lock. Together, `Arc<Mutex<T>>` is the standard way
    // to share mutable state across async tasks — equivalent to Java's
    // `private final ReentrantLock lock = new ReentrantLock()` + synchronized access,
    // but enforced by the type system (you cannot access state without locking).
    let family_id: i64 = 1;
    let history = ConversationState::load_history(&pool, family_id, 50).await?;
    tracing::info!("Loaded {} messages from history", history.len());

    let mut state = ConversationState::with_db(family_id, pool);
    state.messages = history;
    let shared_state = Arc::new(Mutex::new(state));

    // Start the Telegram bot (blocks until shutdown)
    tracing::info!("Starting Telegram bot...");
    home_llm_bot::telegram::start(
        config.telegram_token,
        orchestrator,
        whisper,
        shared_state,
    )
    .await?;

    Ok(())
}
