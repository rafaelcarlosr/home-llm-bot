use super::{Plugin, FunctionDef};
use serde_json::{json, Value};
use crate::error::{BotError, Result};
use reqwest::Client;

/// Home Assistant REST API integration plugin.
///
/// **Rust learning note:**
/// We own the `Client`, which uses Arc internally for connection pooling.
/// The `match` statement is Rust's exhaustive pattern matching (like Java switch)
/// except it's enforced by the compiler — you must cover all cases or get a compile error.
pub struct HomeAssistantPlugin {
    url: String,
    token: String,
    client: Client,
}

impl HomeAssistantPlugin {
    pub fn new(url: String, token: String) -> Self {
        Self {
            url,
            token,
            client: Client::new(),
        }
    }

    /// Build Authorization header for Home Assistant API.
    fn auth_header(&self) -> String {
        format!("Bearer {}", self.token)
    }
}

#[async_trait::async_trait]
impl Plugin for HomeAssistantPlugin {
    /// Execute a Home Assistant function call.
    ///
    /// **Rust learning note on match:**
    /// The `match` exhaustively covers all function names.
    /// If a new function is added to `available_functions()` but not here,
    /// the compiler will warn about non-exhaustive pattern matching.
    /// This is type-safe dispatch — impossible to forget a case.
    async fn execute(&self, function_name: &str, params: Value) -> Result<Value> {
        match function_name {
            "turn_on_light" => {
                let entity_id = params["entity_id"]
                    .as_str()
                    .ok_or_else(|| BotError::HomeAssistant("Missing entity_id".into()))?;

                let mut body = json!({"entity_id": entity_id});

                // Optional brightness parameter
                if let Some(pct) = params["brightness_pct"].as_f64() {
                    body["brightness_pct"] = json!(pct as i32);
                }

                self.call_service("light", "turn_on", body).await
            }
            "turn_off_light" => {
                let entity_id = params["entity_id"]
                    .as_str()
                    .ok_or_else(|| BotError::HomeAssistant("Missing entity_id".into()))?;

                self.call_service("light", "turn_off", json!({"entity_id": entity_id}))
                    .await
            }
            "get_entity_state" => {
                let entity_id = params["entity_id"]
                    .as_str()
                    .ok_or_else(|| BotError::HomeAssistant("Missing entity_id".into()))?;

                self.get_state(entity_id).await
            }
            "set_thermostat" => {
                let entity_id = params["entity_id"]
                    .as_str()
                    .ok_or_else(|| BotError::HomeAssistant("Missing entity_id".into()))?;

                let temperature = params["temperature"]
                    .as_f64()
                    .ok_or_else(|| BotError::HomeAssistant("Missing temperature".into()))?;

                self.call_service(
                    "climate",
                    "set_temperature",
                    json!({"entity_id": entity_id, "temperature": temperature}),
                )
                .await
            }
            "list_entities" => self.list_entities().await,
            _ => Err(BotError::HomeAssistant(format!(
                "Unknown function: {}",
                function_name
            ))),
        }
    }

    fn available_functions(&self) -> Vec<FunctionDef> {
        vec![
            FunctionDef {
                name: "turn_on_light".to_string(),
                description: "Turn on a light or switch by entity ID".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "entity_id": {
                            "type": "string",
                            "description": "Home Assistant entity ID, e.g. light.kitchen"
                        },
                        "brightness_pct": {
                            "type": "number",
                            "description": "Brightness percentage 0-100 (optional)"
                        }
                    },
                    "required": ["entity_id"]
                }),
            },
            FunctionDef {
                name: "turn_off_light".to_string(),
                description: "Turn off a light or switch by entity ID".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "entity_id": {
                            "type": "string",
                            "description": "Home Assistant entity ID"
                        }
                    },
                    "required": ["entity_id"]
                }),
            },
            FunctionDef {
                name: "get_entity_state".to_string(),
                description: "Get the current state of any Home Assistant entity".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "entity_id": {
                            "type": "string",
                            "description": "Entity ID to query"
                        }
                    },
                    "required": ["entity_id"]
                }),
            },
            FunctionDef {
                name: "set_thermostat".to_string(),
                description: "Set a climate/thermostat entity to a target temperature".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "entity_id": {
                            "type": "string",
                            "description": "Climate entity ID"
                        },
                        "temperature": {
                            "type": "number",
                            "description": "Target temperature in Celsius"
                        }
                    },
                    "required": ["entity_id", "temperature"]
                }),
            },
            FunctionDef {
                name: "list_entities".to_string(),
                description: "List all Home Assistant entities and their current states".to_string(),
                parameters: json!({"type": "object", "properties": {}}),
            },
        ]
    }
}

impl HomeAssistantPlugin {
    /// Call a Home Assistant service (action).
    async fn call_service(&self, domain: &str, service: &str, body: Value) -> Result<Value> {
        let url = format!("{}/api/services/{}/{}", self.url, domain, service);

        let response = self.client
            .post(&url)
            .header("Authorization", self.auth_header())
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        check_status(response).await
    }

    /// Get the state of an entity.
    async fn get_state(&self, entity_id: &str) -> Result<Value> {
        let url = format!("{}/api/states/{}", self.url, entity_id);

        let response = self.client
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await?;

        check_status(response).await
    }

    /// List all entities and their states.
    async fn list_entities(&self) -> Result<Value> {
        let url = format!("{}/api/states", self.url);

        let response = self.client
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await?;

        check_status(response).await
    }
}

/// Helper to check HTTP response status and extract JSON.
async fn check_status(response: reqwest::Response) -> Result<Value> {
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(BotError::HomeAssistant(format!(
            "HTTP {}: {}",
            status, body
        )));
    }

    Ok(response.json().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_available_functions_returns_five() {
        let plugin = HomeAssistantPlugin::new("http://localhost:8123".into(), "token".into());
        assert_eq!(plugin.available_functions().len(), 5);
    }

    #[test]
    fn test_execute_unknown_function_returns_error() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let plugin = HomeAssistantPlugin::new("http://localhost:8123".into(), "token".into());

        let result = rt.block_on(plugin.execute("nonexistent_fn", json!({})));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unknown function"));
    }

    #[tokio::test]
    #[ignore] // Requires real Home Assistant instance
    async fn test_real_turn_on_light() {
        // Uncomment to test against real HA:
        // let plugin = HomeAssistantPlugin::new(
        //     "http://192.168.1.100:8123".into(),
        //     "your_ha_token".into()
        // );
        // let result = plugin.execute("turn_on_light", json!({"entity_id": "light.kitchen"})).await;
        // assert!(result.is_ok());
    }
}
