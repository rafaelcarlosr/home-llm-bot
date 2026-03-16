use crate::error::{BotError, Result};
use crate::plugins::{PluginRegistry, lm_studio::LMStudioProvider};
use crate::state::ConversationState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// A function call invoked by the LLM, with parameters to be executed.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FunctionCall {
    pub name: String,
    pub parameters: Value,
}

/// **Rust learning note on ownership:**
/// The `Orchestrator` *owns* the `provider` and `registry`.
/// In Java, we'd inject these as constructor args and store references.
/// In Rust, we own them directly. This is enforced at compile time.
/// When `Orchestrator` is dropped, its owned fields are dropped too.
pub struct Orchestrator {
    provider: LMStudioProvider,
    registry: PluginRegistry,
    model: String,
}

impl Orchestrator {
    pub fn new(
        provider: LMStudioProvider,
        registry: PluginRegistry,
        model: String,
    ) -> Self {
        Self {
            provider,
            registry,
            model,
        }
    }

    /// Process a user message and orchestrate function calls.
    ///
    /// **Two-turn function calling loop:**
    /// 1. Add user message to state
    /// 2. Call LM Studio with available functions
    /// 3. If LM returns function calls → execute each → add results to state
    /// 4. Call LM Studio again with results for a natural language reply
    /// 5. Return final reply to user
    ///
    /// **Rust learning note on `&mut`:**
    /// The `state` parameter is `&mut ConversationState`, meaning we have
    /// exclusive mutable access. The compiler ensures no one else can read/write
    /// the state simultaneously. Similar to synchronized access in Java, but
    /// enforced at compile time with no runtime lock overhead.
    pub async fn process_message(
        &self,
        user_message: &str,
        state: &mut ConversationState,
    ) -> Result<String> {
        // Step 1: Add user message to history
        state.add_message("user", user_message, None);

        // Step 2: Build OpenAI messages array from context window
        let context = state.get_context_window(20);
        let messages: Vec<Value> = context
            .iter()
            .map(|m| json!({"role": m.role, "content": m.content}))
            .collect();

        // Step 3: Call LM Studio with available functions
        let tools: Vec<Value> = self.registry
            .get_all_functions()
            .into_iter()
            .map(|f| f.to_openai_tool())
            .collect();

        let llm_response = self.provider.call_llm(messages.clone(), tools, &self.model).await?;

        // Step 4: Parse and execute any function calls
        let function_calls = parse_function_calls(&llm_response)?;

        if !function_calls.is_empty() {
            // Execute each function call and collect results
            for call in function_calls {
                let result = self.registry.execute(&call.name, call.parameters.clone()).await?;
                let result_str = serde_json::to_string(&result)?;
                // Add function result to state with "tool" role (OpenAI convention)
                state.add_message("tool", &result_str, None);
            }

            // Step 5: Call LM Studio again with tool results for final reply
            let updated_messages: Vec<Value> = state
                .get_context_window(20)
                .iter()
                .map(|m| json!({"role": m.role, "content": m.content}))
                .collect();
            let final_response = self.provider.call_llm(updated_messages, vec![], &self.model).await?;
            let response = extract_text_response(&final_response);
            state.add_message("assistant", &response, None);
            Ok(response)
        } else {
            // No function calls — extract text response directly
            let response = extract_text_response(&llm_response);
            state.add_message("assistant", &response, None);
            Ok(response)
        }
    }
}

/// Parse LM Studio's function call response in OpenAI format.
///
/// **Rust learning note on `Option` chaining:**
/// The `response["choices"][0]["message"]["tool_calls"]` chain uses
/// implicit `Option` operations. If any step is None, we return `Ok(vec![])`.
/// In Java we'd write:
/// ```java
/// if (response.has("choices") && response.getJSONArray("choices").length() > 0) {
///     var calls = response.getJSONArray("choices").getJSONObject(0)...
/// }
/// ```
/// Rust's `Option` makes the pattern explicit and chainable.
fn parse_function_calls(response: &Value) -> Result<Vec<FunctionCall>> {
    let tool_calls = response["choices"][0]["message"]["tool_calls"]
        .as_array();

    let Some(calls) = tool_calls else {
        return Ok(vec![]);
    };

    calls
        .iter()
        .map(|tc| {
            let name = tc["function"]["name"]
                .as_str()
                .ok_or_else(|| BotError::LMStudio("Missing function name in tool_call".into()))?
                .to_string();

            let args_str = tc["function"]["arguments"]
                .as_str()
                .ok_or_else(|| BotError::LMStudio("Missing function arguments".into()))?;

            let parameters: Value = serde_json::from_str(args_str)?;

            Ok(FunctionCall { name, parameters })
        })
        .collect()
}

/// Extract the text response from LM Studio's response.
fn extract_text_response(response: &Value) -> String {
    response["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_function_call_response_with_tools() {
        // **Rust learning note on test JSON construction:**
        // `json!()` macro creates a serde_json::Value inline.
        // This is similar to Java's JSONObject or Kotlin's mapOf.
        let response = json!({
            "choices": [{
                "message": {
                    "tool_calls": [{
                        "id": "call_abc123",
                        "type": "function",
                        "function": {
                            "name": "turn_on_light",
                            "arguments": "{\"entity_id\": \"light.kitchen\"}"
                        }
                    }]
                }
            }]
        });

        let calls = parse_function_calls(&response).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "turn_on_light");
        assert_eq!(calls[0].parameters["entity_id"], "light.kitchen");
    }

    #[test]
    fn test_parse_function_call_response_no_tools() {
        let response = json!({
            "choices": [{
                "message": {
                    "content": "No function needed"
                }
            }]
        });

        let calls = parse_function_calls(&response).unwrap();
        assert_eq!(calls.len(), 0);
    }

    #[test]
    fn test_extract_text_response() {
        let response = json!({
            "choices": [{
                "message": {
                    "content": "Lights are on!"
                }
            }]
        });

        assert_eq!(extract_text_response(&response), "Lights are on!");
    }

    #[test]
    fn test_extract_text_response_with_null_content() {
        // Tool call response has null content
        let response = json!({
            "choices": [{
                "message": {
                    "content": null,
                    "tool_calls": []
                }
            }]
        });

        assert_eq!(extract_text_response(&response), "");
    }
}
