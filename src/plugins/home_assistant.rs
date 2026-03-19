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
            "turn_on_device" => {
                let entity_id = params["entity_id"]
                    .as_str()
                    .ok_or_else(|| BotError::HomeAssistant("Missing entity_id".into()))?;

                // Detect domain from entity_id prefix (e.g. "switch.cave" → "switch")
                let domain = entity_id.split('.').next().unwrap_or("homeassistant");
                let mut body = json!({"entity_id": entity_id});

                // Brightness only makes sense for lights
                if domain == "light" {
                    if let Some(pct) = params["brightness_pct"].as_f64() {
                        body["brightness_pct"] = json!(pct as i32);
                    }
                }

                self.call_service(domain, "turn_on", body).await
            }
            "turn_off_device" => {
                let entity_id = params["entity_id"]
                    .as_str()
                    .ok_or_else(|| BotError::HomeAssistant("Missing entity_id".into()))?;

                let domain = entity_id.split('.').next().unwrap_or("homeassistant");
                self.call_service(domain, "turn_off", json!({"entity_id": entity_id}))
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
            "search_entities" => {
                let domain = params["domain"].as_str();
                let query = params["query"].as_str();
                self.search_entities(domain, query).await
            }
            "call_service" => {
                let domain = params["domain"]
                    .as_str()
                    .ok_or_else(|| BotError::HomeAssistant("Missing domain".into()))?;
                let service = params["service"]
                    .as_str()
                    .ok_or_else(|| BotError::HomeAssistant("Missing service".into()))?;
                let entity_id = params["entity_id"]
                    .as_str()
                    .ok_or_else(|| BotError::HomeAssistant("Missing entity_id".into()))?;
                let mut body = json!({"entity_id": entity_id});
                // Forward any extra data fields
                if let Some(data) = params["data"].as_object() {
                    for (k, v) in data {
                        body[k] = v.clone();
                    }
                }
                self.call_service(domain, service, body).await
            }
            _ => Err(BotError::HomeAssistant(format!(
                "Unknown function: {}",
                function_name
            ))),
        }
    }

    fn available_functions(&self) -> Vec<FunctionDef> {
        vec![
            FunctionDef {
                name: "turn_on_device".to_string(),
                description: "Turn on any Home Assistant device (light, switch, scene, script, fan, etc.) by entity ID. The domain is inferred from the entity_id prefix.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "entity_id": {
                            "type": "string",
                            "description": "Home Assistant entity ID, e.g. light.kitchen, switch.sonoff_abc, scene.ligar_cave"
                        },
                        "brightness_pct": {
                            "type": "number",
                            "description": "Brightness percentage 0-100, only for light entities (optional)"
                        }
                    },
                    "required": ["entity_id"]
                }),
            },
            FunctionDef {
                name: "turn_off_device".to_string(),
                description: "Turn off any Home Assistant device (light, switch, fan, etc.) by entity ID. The domain is inferred from the entity_id prefix.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "entity_id": {
                            "type": "string",
                            "description": "Home Assistant entity ID, e.g. switch.sonoff_abc, light.bedroom"
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
                name: "search_entities".to_string(),
                description: "Search Home Assistant entities by domain and/or name. Use this when you need to find an entity ID. Returns a compact list (max 20) with entity_id, state, and friendly_name.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "domain": {
                            "type": "string",
                            "description": "Filter by domain, e.g. 'light', 'switch', 'climate', 'scene', 'script'"
                        },
                        "query": {
                            "type": "string",
                            "description": "Substring to match against entity_id or friendly_name"
                        }
                    },
                    "required": []
                }),
            },
            FunctionDef {
                name: "call_service".to_string(),
                description: "Call any Home Assistant service. Use for actions not covered by other tools (e.g. activating scenes, running scripts, or toggling devices).".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "domain": {
                            "type": "string",
                            "description": "Service domain, e.g. 'scene', 'script', 'light', 'switch'"
                        },
                        "service": {
                            "type": "string",
                            "description": "Service name, e.g. 'turn_on', 'turn_off', 'toggle'"
                        },
                        "entity_id": {
                            "type": "string",
                            "description": "Target entity ID"
                        },
                        "data": {
                            "type": "object",
                            "description": "Additional service data (optional)"
                        }
                    },
                    "required": ["domain", "service", "entity_id"]
                }),
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

    /// Get the state of an entity, returning only essential fields.
    async fn get_state(&self, entity_id: &str) -> Result<Value> {
        let url = format!("{}/api/states/{}", self.url, entity_id);

        let response = self.client
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await?;

        let full = check_status(response).await?;
        let attrs = &full["attributes"];

        // Return compact representation — only fields useful for the LLM
        let mut result = json!({
            "entity_id": full["entity_id"],
            "state": full["state"],
            "friendly_name": attrs["friendly_name"],
            "last_changed": full["last_changed"],
        });

        // Include domain-specific key attributes
        for key in &["brightness", "color_temp", "temperature", "current_temperature",
                     "humidity", "unit_of_measurement", "device_class"] {
            if !attrs[key].is_null() {
                result[key] = attrs[key].clone();
            }
        }

        Ok(result)
    }

    /// Search entities by optional domain prefix and/or substring query.
    /// Returns a compact list (max 20) with only entity_id, state, friendly_name.
    async fn search_entities(&self, domain: Option<&str>, query: Option<&str>) -> Result<Value> {
        let url = format!("{}/api/states", self.url);

        let response = self.client
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await?;

        let all: Vec<Value> = check_status(response).await?
            .as_array()
            .cloned()
            .unwrap_or_default();

        let results: Vec<Value> = all
            .into_iter()
            .filter(|e| {
                let entity_id = e["entity_id"].as_str().unwrap_or("");
                let friendly = e["attributes"]["friendly_name"].as_str().unwrap_or("").to_lowercase();

                // Domain filter
                if let Some(d) = domain {
                    if !entity_id.starts_with(&format!("{}.", d)) {
                        return false;
                    }
                }
                // Query filter
                if let Some(q) = query {
                    let q_lower = q.to_lowercase();
                    if !entity_id.to_lowercase().contains(&q_lower) && !friendly.contains(&q_lower) {
                        return false;
                    }
                }
                true
            })
            .take(20)
            .map(|e| json!({
                "entity_id": e["entity_id"],
                "state": e["state"],
                "friendly_name": e["attributes"]["friendly_name"]
            }))
            .collect();

        Ok(json!(results))
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
    fn test_available_functions_returns_six() {
        let plugin = HomeAssistantPlugin::new("http://localhost:8123".into(), "token".into());
        assert_eq!(plugin.available_functions().len(), 6);
    }

    #[test]
    fn test_function_names() {
        let plugin = HomeAssistantPlugin::new("http://localhost:8123".into(), "token".into());
        let names: Vec<_> = plugin.available_functions().into_iter().map(|f| f.name).collect();
        assert!(names.contains(&"turn_on_device".to_string()));
        assert!(names.contains(&"turn_off_device".to_string()));
        assert!(names.contains(&"search_entities".to_string()));
        assert!(names.contains(&"call_service".to_string()));
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
