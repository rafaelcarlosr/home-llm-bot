use super::{Plugin, FunctionDef};
use crate::error::{BotError, Result};
use reqwest::Client;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};

/// Home Assistant MCP plugin — dynamically discovers and calls HA tools via the
/// official MCP server at `/api/mcp`.
///
/// **Why MCP over custom REST?**
/// - `HassTurnOn/Off` accepts `name` or `area` (human-readable), so the LLM can say
///   `name: "cave"` and HA resolves the correct entities automatically.
/// - Tools are discovered at startup — no hardcoding entity domains or service names.
/// - Custom scripts and scenes are automatically exposed (e.g. `ligaluzporaonoite`).
/// - `GetLiveContext` provides a compact area-organized state summary.
///
/// **Rust learning note on async constructor:**
/// Rust constructors (`new`) are synchronous by convention. When initialization
/// requires I/O (here: fetching the tool list from HA), we use an async `init()`
/// factory function instead. The caller awaits it: `McpPlugin::init(...).await?`.
pub struct McpPlugin {
    url: String,
    token: String,
    client: Client,
    tools: Vec<FunctionDef>,
    /// Monotonically increasing JSON-RPC request id.
    /// AtomicU64 gives us interior mutability without a Mutex (safe across async tasks).
    request_id: AtomicU64,
}

impl McpPlugin {
    /// Initialise the plugin by fetching the tool list from the HA MCP server.
    /// Returns an error if the server is unreachable or returns no tools.
    pub async fn init(ha_url: String, token: String) -> Result<Self> {
        let client = Client::new();
        let url = format!("{}/api/mcp", ha_url);

        let response: Value = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/list",
                "params": {}
            }))
            .send()
            .await
            .map_err(BotError::Http)?
            .json()
            .await
            .map_err(BotError::Http)?;

        let tools_raw = response["result"]["tools"]
            .as_array()
            .ok_or_else(|| BotError::HomeAssistant("MCP tools/list returned no tools array".into()))?;

        let tools: Vec<FunctionDef> = tools_raw
            .iter()
            .map(|t| FunctionDef {
                name: t["name"].as_str().unwrap_or("unknown").to_string(),
                description: t["description"].as_str().unwrap_or("").to_string(),
                // inputSchema is already valid JSON Schema — maps directly to OpenAI tool parameters
                parameters: t["inputSchema"].clone(),
            })
            .collect();

        tracing::info!("MCP plugin initialised with {} tools", tools.len());

        Ok(Self {
            url,
            token,
            client,
            tools,
            request_id: AtomicU64::new(2),
        })
    }
}

#[async_trait::async_trait]
impl Plugin for McpPlugin {
    async fn execute(&self, function_name: &str, params: Value) -> Result<Value> {
        let id = self.request_id.fetch_add(1, Ordering::Relaxed);

        let response: Value = self
            .client
            .post(&self.url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "tools/call",
                "params": {
                    "name": function_name,
                    "arguments": params
                }
            }))
            .send()
            .await
            .map_err(BotError::Http)?
            .json()
            .await
            .map_err(BotError::Http)?;

        // Surface JSON-RPC errors as BotError
        if let Some(err) = response.get("error") {
            return Err(BotError::HomeAssistant(format!("MCP error: {}", err)));
        }

        // Extract text content from result.content[0].text
        let content = &response["result"]["content"];
        if let Some(text) = content.as_array()
            .and_then(|arr| arr.first())
            .and_then(|item| item["text"].as_str())
        {
            // For GetLiveContext, filter the large result to only useful entries
            let processed = if function_name == "GetLiveContext" {
                filter_live_context(text)
            } else {
                text.to_string()
            };
            // Try to parse as JSON; if it's plain text, wrap it
            return Ok(serde_json::from_str(&processed)
                .unwrap_or_else(|_| json!({ "result": processed })));
        }

        Ok(json!({ "result": "ok" }))
    }

    fn available_functions(&self) -> Vec<FunctionDef> {
        self.tools.clone()
    }
}

/// Filter the GetLiveContext YAML to remove noise before sending to the LLM.
///
/// HA's GetLiveContext returns ~10KB with 90+ entries (AdGuard switches, Solarman
/// sensors, unavailable devices, etc.). This drops irrelevant entries so the LLM
/// can answer "is the AC on?" without getting lost in a wall of unrelated data.
///
/// The input text from MCP is `{"success": true, "result": "Live Context: ...YAML..."}`.
/// We extract the YAML, filter it, and rewrap it.
fn filter_live_context(raw: &str) -> String {
    const SKIP_NAMES: &[&str] = &[
        "adguard", "disjuntor", "solarman", "xiaomi", "pm10", "pm2.5",
        "smartthinq", "icloud", "truenas", "plex",
    ];

    // The MCP text content is JSON: {"success": true, "result": "Live Context: ...\n- names:..."}
    // Extract the inner YAML from the "result" field
    let yaml = if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        v["result"].as_str().unwrap_or(raw).to_string()
    } else {
        raw.to_string()
    };

    // Split into header + per-device blocks (each block starts with "- names:")
    let mut header_lines: Vec<&str> = Vec::new();
    let mut blocks: Vec<String> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let mut in_blocks = false;

    for line in yaml.lines() {
        if line.trim_start().starts_with("- names:") {
            if !current.is_empty() {
                blocks.push(current.join("\n"));
                current.clear();
            }
            in_blocks = true;
        }
        if in_blocks {
            current.push(line);
        } else {
            header_lines.push(line);
        }
    }
    if !current.is_empty() {
        blocks.push(current.join("\n"));
    }

    let filtered: Vec<String> = blocks
        .into_iter()
        .filter(|block| {
            let lower = block.to_lowercase();
            // Drop unavailable / unknown state
            if lower.contains("state: 'unavailable'") || lower.contains("state: unavailable")
                || lower.contains("state: 'unknown'") || lower.contains("state: unknown")
            {
                return false;
            }
            // Drop noisy infrastructure devices by name keyword
            if SKIP_NAMES.iter().any(|skip| lower.contains(skip)) {
                return false;
            }
            true
        })
        .collect();

    let filtered_yaml = format!("{}\n{}", header_lines.join("\n"), filtered.join("\n"));
    // Return as the same JSON shape HA sends: {"success": true, "result": "..."}
    serde_json::to_string(&serde_json::json!({
        "success": true,
        "result": filtered_yaml
    }))
    .unwrap_or(filtered_yaml)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function_def_maps_input_schema() {
        // Verify that inputSchema from MCP maps directly to FunctionDef.parameters
        let input_schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "area": { "type": "string" }
            }
        });

        let def = FunctionDef {
            name: "HassTurnOn".to_string(),
            description: "Turns on a device".to_string(),
            parameters: input_schema.clone(),
        };

        assert_eq!(def.name, "HassTurnOn");
        assert_eq!(def.parameters["properties"]["name"]["type"], "string");
    }

    #[tokio::test]
    #[ignore] // Requires real HA instance
    async fn test_real_mcp_init() {
        // let plugin = McpPlugin::init(
        //     "http://192.168.0.60:8123".to_string(),
        //     "your_token".to_string(),
        // ).await.unwrap();
        // assert!(!plugin.available_functions().is_empty());
    }
}
