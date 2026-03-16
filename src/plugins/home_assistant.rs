use super::{Plugin, FunctionDef};
use serde_json::{json, Value};
use crate::error::Result;

pub struct HomeAssistantPlugin {
    #[allow(dead_code)]
    url: String,
    #[allow(dead_code)]
    token: String,
}

impl HomeAssistantPlugin {
    pub fn new(url: String, token: String) -> Self {
        Self { url, token }
    }
}

#[async_trait::async_trait]
impl Plugin for HomeAssistantPlugin {
    async fn execute(&self, _function_name: &str, _params: Value) -> Result<Value> {
        // TODO: implement
        Ok(json!({"status": "ok"}))
    }

    fn available_functions(&self) -> Vec<FunctionDef> {
        vec![
            FunctionDef {
                name: "turn_on_light".to_string(),
                description: "Turn on a light by entity ID".to_string(),
                parameters: json!({"entity_id": "string"}),
            },
            FunctionDef {
                name: "turn_off_light".to_string(),
                description: "Turn off a light by entity ID".to_string(),
                parameters: json!({"entity_id": "string"}),
            },
        ]
    }
}
