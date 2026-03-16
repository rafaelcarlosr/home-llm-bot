pub mod home_assistant;
pub mod lm_studio;
pub mod whisper;

use serde_json::Value;
use crate::error::Result;

#[async_trait::async_trait]
pub trait Plugin: Send + Sync {
    async fn execute(&self, function_name: &str, params: Value) -> Result<Value>;
    fn available_functions(&self) -> Vec<FunctionDef>;
}

#[derive(Clone, Debug)]
pub struct FunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: Value,
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
