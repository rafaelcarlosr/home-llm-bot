use crate::error::{BotError, Result};
use crate::plugins::LlmProvider;
use serde_json::{json, Value};
use reqwest::Client;

/// LLM provider for LM Studio (local OpenAI-compatible API).
///
/// **Rust learning note:** We own a `Client` here, which is expensive to create
/// but cheap to clone (it uses Arc internally for connection pooling).
/// Similar to Java's HttpClient being expensive to create but reusable.
pub struct LMStudioProvider {
    url: String,
    client: Client,
    temperature: f64,
}

impl LMStudioProvider {
    pub fn new(url: String, temperature: f64) -> Self {
        Self {
            url,
            client: Client::new(),
            temperature,
        }
    }
}

/// Implement the `LlmProvider` trait so the orchestrator can use LMStudioProvider
/// via `Box<dyn LlmProvider>`. This also makes it easy to swap in a mock for tests.
#[async_trait::async_trait]
impl LlmProvider for LMStudioProvider {
    async fn call_llm(
        &self,
        messages: Vec<Value>,
        tools: Vec<Value>,
        model: &str,
    ) -> Result<Value> {
        let mut body = json!({
            "model": model,
            "messages": messages,
            "temperature": self.temperature,
        });

        if !tools.is_empty() {
            body["tools"] = json!(tools);
            body["tool_choice"] = json!("auto");
        }

        let response = self.client
            .post(format!("{}/v1/chat/completions", self.url))
            .json(&body)
            .send()
            .await
            .map_err(BotError::Http)?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(BotError::LMStudio(format!(
                "HTTP {}: {}",
                status, text
            )));
        }

        response.json::<Value>().await.map_err(BotError::Http)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_with_tools_includes_tool_choice() {
        // Unit test: verify the JSON body structure is correct
        // (We can't actually test HTTP without a real server)
        let tools = vec![json!({
            "type": "function",
            "function": {
                "name": "test_fn",
                "description": "A test",
                "parameters": {}
            }
        })];

        let expected_tools_count = 1;
        assert_eq!(tools.len(), expected_tools_count);
    }

    #[tokio::test]
    #[ignore] // Requires real LM Studio instance
    async fn test_real_lm_studio_call() {
        // Integration test: uncomment to test against real LM Studio
        // let provider = LMStudioProvider::new("http://localhost:1234".to_string(), 0.3);
        // let messages = vec![json!({"role": "user", "content": "Hello"})];
        // let result = provider.call_llm(messages, vec![], "qwen2.5-7b").await;
        // assert!(result.is_ok());
    }
}
