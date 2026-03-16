use crate::error::Result;
use crate::plugins::PluginRegistry;
use crate::state::ConversationState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Serialize, Deserialize, Debug)]
pub struct FunctionCall {
    pub name: String,
    pub parameters: Value,
}

pub struct Orchestrator {
    lm_studio_url: String,
    registry: PluginRegistry,
}

impl Orchestrator {
    pub fn new(lm_studio_url: String, registry: PluginRegistry) -> Self {
        Self {
            lm_studio_url,
            registry,
        }
    }

    pub async fn process_message(
        &self,
        user_message: &str,
        state: &mut ConversationState,
    ) -> Result<String> {
        // Add user message to history
        state.add_message("user", user_message, None);

        // Build prompt with available functions
        let available_functions = self.registry.get_all_functions();
        let functions_json = serde_json::to_string(&available_functions)?;
        let context = state.get_context_window(10);

        let prompt = format!(
            "You are a helpful home automation assistant. Available functions: {}\n\nConversation:\n{:?}\n\nRespond with function calls if needed.",
            functions_json, context
        );

        // Call LM Studio
        let llm_response = self.call_lm_studio(&prompt).await?;

        // Parse and execute function calls
        let function_calls = parse_function_calls(&llm_response)?;
        for call in function_calls {
            let result = self.registry.execute(&call.name, call.parameters).await?;
            state.add_message("function", &format!("{}: {}", call.name, result), None);
        }

        // Extract text response
        let response = extract_text_response(&llm_response);
        state.add_message("assistant", &response, None);

        Ok(response)
    }

    async fn call_lm_studio(&self, prompt: &str) -> Result<Value> {
        let client = reqwest::Client::new();
        let body = json!({
            "messages": [{"role": "user", "content": prompt}],
            "temperature": 0.7,
        });

        let response = client
            .post(&format!("{}/v1/chat/completions", self.lm_studio_url))
            .json(&body)
            .send()
            .await?;

        let json = response.json().await?;
        Ok(json)
    }
}

fn parse_function_calls(_response: &Value) -> Result<Vec<FunctionCall>> {
    // TODO: Parse LM Studio response for function calls
    Ok(vec![])
}

fn extract_text_response(_response: &Value) -> String {
    // TODO: Extract text from LM Studio response
    "Response".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_function_call_response() {
        let response = json!({
            "function_calls": [
                {"name": "turn_on_light", "parameters": {"entity_id": "light.kitchen"}}
            ]
        });

        let calls = parse_function_calls(&response).unwrap();
        assert_eq!(calls.len(), 0); // TODO: will be 1 once parsing is implemented
    }
}
