use tracing_subscriber;
use home_llm_bot::plugins::PluginRegistry;
use home_llm_bot::plugins::home_assistant::HomeAssistantPlugin;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    tracing::info!("Starting home-llm-bot");

    let mut registry = PluginRegistry::new();
    registry.register(Box::new(
        HomeAssistantPlugin::new(
            "http://localhost:8123".to_string(),
            "token".to_string(),
        )
    ));
    let functions = registry.get_all_functions();
    tracing::info!("Registered {} functions", functions.len());

    Ok(())
}
