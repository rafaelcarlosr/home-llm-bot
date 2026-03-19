pub mod home_assistant;
pub mod lm_studio;
pub mod mcp;
pub mod whisper;

use serde_json::Value;
use serde::{Deserialize, Serialize};
use crate::error::Result;

/// Abstraction over the LLM backend (LM Studio, OpenAI, etc.).
///
/// **Rust learning note on trait objects:**
/// This trait is used as `Box<dyn LlmProvider>` — a heap-allocated trait object.
/// Think of it like a Java interface: `LlmProvider provider = new LMStudioProvider(...)`.
/// The `Send + Sync` bounds mean it's safe to share across async tasks (threads).
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    async fn call_llm(&self, messages: Vec<Value>, tools: Vec<Value>, model: &str) -> Result<Value>;
}

#[async_trait::async_trait]
pub trait Plugin: Send + Sync {
    async fn execute(&self, function_name: &str, params: Value) -> Result<Value>;
    fn available_functions(&self) -> Vec<FunctionDef>;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

impl FunctionDef {
    /// Convert to OpenAI-compatible tool object for the API request.
    ///
    /// **Rust learning note:** This `impl` block adds methods to the struct.
    /// In Java, this would be like adding a method directly in the class.
    /// In Rust, you can have multiple `impl` blocks for the same struct.
    pub fn to_openai_tool(&self) -> Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters
            }
        })
    }
}

pub struct PluginRegistry {
    plugins: Vec<Box<dyn Plugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        self.plugins.push(plugin);
    }

    pub fn get_all_functions(&self) -> Vec<FunctionDef> {
        self.plugins
            .iter()
            .flat_map(|p| p.available_functions())
            .collect()
    }

    pub async fn execute(&self, function_name: &str, params: Value) -> Result<Value> {
        for plugin in &self.plugins {
            if plugin.available_functions().iter().any(|f| f.name == function_name) {
                return plugin.execute(function_name, params).await;
            }
        }
        Err(crate::error::BotError::Config(format!("Function not found: {}", function_name)))
    }
}
