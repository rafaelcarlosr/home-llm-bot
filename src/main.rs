use dotenv::dotenv;
use home_llm_bot::{
    config::Config,
    orchestrator::Orchestrator,
    plugins::{
        PluginRegistry,
        mcp::McpPlugin,
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

    // Build plugin registry using the HA MCP server.
    // McpPlugin::init() fetches the tool list from HA at startup — no hardcoded tools.
    // **Rust learning note on async constructors:**
    // Rust `new()` is synchronous by convention. We use an async `init()` factory
    // when initialisation requires I/O (here: fetching tools from Home Assistant).
    let mut registry = PluginRegistry::new();
    let mcp = McpPlugin::init(
        config.home_assistant_url.clone(),
        config.home_assistant_token.clone(),
    )
    .await?;
    registry.register(Box::new(mcp));
    tracing::info!("Registered {} MCP functions from Home Assistant", registry.get_all_functions().len());

    // LLM model name — defaults to a sensible local model if not set
    let model = std::env::var("LLM_MODEL")
        .unwrap_or_else(|_| "qwen2.5-7b-instruct".to_string());

    // Optional entity hints for the system prompt, e.g.:
    // ENTITY_HINTS="switch.sonoff_1000ab571c=Luz Cave (porão externo)\nscene.ligar_cave=Ligar Cave"
    let entity_hints = std::env::var("ENTITY_HINTS").ok();

    // Build the orchestrator (owns the LLM provider and plugin registry)
    let lm_provider = LMStudioProvider::new(config.lm_studio_url.clone());
    let orchestrator = Arc::new(Orchestrator::new(Box::new(lm_provider), registry, model, entity_hints));

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
