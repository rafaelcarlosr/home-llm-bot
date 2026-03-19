use crate::error::{BotError, Result};
use crate::plugins::{LlmProvider, PluginRegistry};
use crate::state::ConversationState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{info, warn};

/// Maximum number of tool-call round trips before forcing a text response.
/// Prevents infinite loops if the LLM keeps requesting tools.
const MAX_TOOL_ITERATIONS: usize = 5;

/// A function call requested by the LLM, with an id to correlate the result.
///
/// **Rust learning note:** The `id` field comes from the OpenAI `tool_call` object.
/// When we send back the tool result, we must include the matching `tool_call_id`
/// so the LLM knows which result corresponds to which call.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FunctionCall {
    pub id: String,
    pub name: String,
    pub parameters: Value,
}

/// The orchestrator connects the LLM provider with the plugin registry.
///
/// **Rust learning note on trait objects (`Box<dyn LlmProvider>`):**
/// Instead of owning a concrete `LMStudioProvider`, we hold a `Box<dyn LlmProvider>`.
/// This is like Java's `LlmProvider provider` interface field — we can swap in any
/// implementation (real HTTP provider, or a mock for tests) without changing Orchestrator.
pub struct Orchestrator {
    provider: Box<dyn LlmProvider>,
    registry: PluginRegistry,
    model: String,
    /// Optional hints about known entities in this home, injected into the system prompt.
    /// Set via ENTITY_HINTS env var, e.g. "switch.sonoff_abc=Cave light,light.kitchen=Kitchen"
    entity_hints: Option<String>,
}

impl Orchestrator {
    pub fn new(
        provider: Box<dyn LlmProvider>,
        registry: PluginRegistry,
        model: String,
        entity_hints: Option<String>,
    ) -> Self {
        Self {
            provider,
            registry,
            model,
            entity_hints,
        }
    }

    /// Process a user message using an agentic loop that supports chained tool calls.
    ///
    /// **Flow:**
    /// 1. Add user message to persistent state
    /// 2. Build a working `messages` vec (OpenAI format) from the context window
    /// 3. Build the tools array from the plugin registry
    /// 4. **Loop** (up to `MAX_TOOL_ITERATIONS`):
    ///    a. Call the LLM with messages + tools
    ///    b. If the LLM returns `tool_calls`: push the assistant message (with tool_calls)
    ///       into the working vec, execute each tool, push tool-role results with matching
    ///       `tool_call_id` -> continue loop
    ///    c. If no `tool_calls`: extract text, persist to state, return
    /// 5. If the loop is exhausted: make one final call with empty tools to force a text reply
    ///
    /// **Why a working vec?** During the loop we accumulate assistant+tool messages that
    /// the LLM needs to see on the next iteration. We also persist them to state for history.
    pub async fn process_message(
        &self,
        user_message: &str,
        sender_name: Option<&str>,
        state: &mut ConversationState,
    ) -> Result<String> {
        // Step 1: Build context BEFORE adding user message (to avoid double-counting)
        let system_prompt = build_system_prompt(self.entity_hints.as_deref());
        let context = state.get_context_window(20);
        let mut messages: Vec<Value> = std::iter::once(json!({"role": "system", "content": system_prompt}))
            .chain(context.iter().map(|m| {
                let content = match &m.sender_name {
                    Some(name) if m.role == "user" => format!("[{}] {}", name, m.content),
                    _ => m.content.clone(),
                };
                json!({"role": m.role, "content": content})
            }))
            .collect();

        // Step 2: Persist user message (adds to state.messages AFTER context snapshot)
        state.add_message_persisted(
            "user",
            user_message,
            sender_name.map(|s| s.to_string()),
        ).await?;

        // Append current user message to working messages
        let user_content = match sender_name {
            Some(name) => format!("[{}] {}", name, user_message),
            None => user_message.to_string(),
        };
        messages.push(json!({"role": "user", "content": user_content}));

        // Step 3: Build tools array from registry
        let tools: Vec<Value> = self.registry
            .get_all_functions()
            .into_iter()
            .map(|f| f.to_openai_tool())
            .collect();

        // Step 4: Agentic loop
        for iteration in 0..MAX_TOOL_ITERATIONS {
            let llm_response = self.provider
                .call_llm(messages.clone(), tools.clone(), &self.model)
                .await?;

            let function_calls = parse_function_calls(&llm_response)?;

            if function_calls.is_empty() {
                // No tool calls — the LLM produced a final text response
                let response = extract_text_response(&llm_response);
                state.add_message_persisted("assistant", &response, None).await?;
                return Ok(response);
            }

            info!(iteration, num_calls = function_calls.len(), "LLM requested tool calls");

            // Push the full assistant message (with tool_calls) into working messages.
            // The OpenAI API requires this so the LLM can see what it previously requested.
            let assistant_msg = llm_response["choices"][0]["message"].clone();
            messages.push(assistant_msg);

            // Execute each tool call and push results
            for call in &function_calls {
                info!(tool = %call.name, params = %call.parameters, "Calling tool");
                let result = self.registry.execute(&call.name, call.parameters.clone()).await;

                let result_str = match &result {
                    Ok(v) => {
                        info!(tool = %call.name, result = %v, "Tool result");
                        serde_json::to_string(v)?
                    }
                    Err(e) => {
                        warn!(tool = %call.name, error = %e, "Tool error");
                        format!("Error: {}", e)
                    }
                };

                // Add tool result to working messages with the matching tool_call_id
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": call.id,
                    "content": result_str,
                }));

                // Also persist to state for conversation history
                state.add_message("tool", &result_str, None);
            }
        }

        // Step 5: Loop exhausted — force a text response by calling without tools
        warn!("Agentic loop reached MAX_TOOL_ITERATIONS ({}), forcing text response", MAX_TOOL_ITERATIONS);
        let final_response = self.provider
            .call_llm(messages, vec![], &self.model)
            .await?;
        let response = extract_text_response(&final_response);
        state.add_message_persisted("assistant", &response, None).await?;
        Ok(response)
    }
}

/// Build the system prompt that guides the LLM on tool usage and home entity conventions.
fn build_system_prompt(entity_hints: Option<&str>) -> String {
    let mut prompt = String::from(
        "You are a home automation assistant for a family. \
        You control devices via Home Assistant tools.\
        \n\nIMPORTANT RULES:\
        \n- Always respond in the same language the user is speaking (Portuguese, English, etc.).\
        \n- To turn on/off devices use HassTurnOn or HassTurnOff with the 'name' parameter \
        (the device's friendly name, e.g. 'Sala 1', 'Ar Alice') or the 'area' parameter \
        (room name, e.g. 'Cave', 'Quarto Alice', 'Sala'). HA will resolve the correct entities.\
        \n- Use GetLiveContext to check the current state of devices before acting when needed.\
        \n- Keep responses short and conversational.",
    );

    if let Some(extra) = entity_hints {
        prompt.push_str("\n\n");
        prompt.push_str(extra);
    }

    prompt
}

/// Parse LM Studio's function call response in OpenAI format.
/// Now also extracts the `id` field from each tool_call for proper correlation.
fn parse_function_calls(response: &Value) -> Result<Vec<FunctionCall>> {
    let tool_calls = response["choices"][0]["message"]["tool_calls"]
        .as_array();

    let Some(calls) = tool_calls else {
        return Ok(vec![]);
    };

    // Filter out empty arrays (some LLMs return `"tool_calls": []` with text)
    if calls.is_empty() {
        return Ok(vec![]);
    }

    calls
        .iter()
        .map(|tc| {
            let id = tc["id"]
                .as_str()
                .unwrap_or("unknown")
                .to_string();

            let name = tc["function"]["name"]
                .as_str()
                .ok_or_else(|| BotError::LMStudio("Missing function name in tool_call".into()))?
                .to_string();

            let args_str = tc["function"]["arguments"]
                .as_str()
                .ok_or_else(|| BotError::LMStudio("Missing function arguments".into()))?;

            let parameters: Value = serde_json::from_str(args_str)?;

            Ok(FunctionCall { id, name, parameters })
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
    use crate::plugins::{LlmProvider, Plugin, FunctionDef, PluginRegistry};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // ---- Mock LLM Provider ----

    /// A mock LLM provider that returns pre-configured responses in sequence.
    /// Each call to `call_llm` returns the next response from the list.
    ///
    /// **Rust learning note on `AtomicUsize`:**
    /// We need interior mutability because `call_llm` takes `&self` (shared ref).
    /// `AtomicUsize` lets us increment a counter without `&mut self`.
    /// In Java this would be `AtomicInteger`.
    struct MockLlmProvider {
        responses: Vec<Value>,
        call_count: AtomicUsize,
    }

    impl MockLlmProvider {
        fn new(responses: Vec<Value>) -> Self {
            Self {
                responses,
                call_count: AtomicUsize::new(0),
            }
        }

        fn get_call_count(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for MockLlmProvider {
        async fn call_llm(&self, _messages: Vec<Value>, _tools: Vec<Value>, _model: &str) -> Result<Value> {
            let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
            if idx < self.responses.len() {
                Ok(self.responses[idx].clone())
            } else {
                // If we run out of responses, return a simple text response
                Ok(json!({
                    "choices": [{"message": {"content": "fallback response"}}]
                }))
            }
        }
    }

    // ---- Mock Plugin ----

    /// A mock plugin that returns a fixed result for any function call.
    struct MockPlugin {
        functions: Vec<FunctionDef>,
    }

    #[async_trait::async_trait]
    impl Plugin for MockPlugin {
        async fn execute(&self, _function_name: &str, _params: Value) -> Result<Value> {
            Ok(json!({"status": "ok"}))
        }

        fn available_functions(&self) -> Vec<FunctionDef> {
            self.functions.clone()
        }
    }

    fn make_registry() -> PluginRegistry {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(MockPlugin {
            functions: vec![
                FunctionDef {
                    name: "list_entities".to_string(),
                    description: "List HA entities".to_string(),
                    parameters: json!({"type": "object", "properties": {}}),
                },
                FunctionDef {
                    name: "turn_off_light".to_string(),
                    description: "Turn off a light".to_string(),
                    parameters: json!({"type": "object", "properties": {"entity_id": {"type": "string"}}}),
                },
            ],
        }));
        registry
    }

    /// Helper to build an OpenAI-format tool_calls response.
    fn tool_calls_response(calls: Vec<(&str, &str, &str)>) -> Value {
        let tool_calls: Vec<Value> = calls
            .into_iter()
            .map(|(id, name, args)| {
                json!({
                    "id": id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": args
                    }
                })
            })
            .collect();

        json!({
            "choices": [{
                "message": {
                    "content": null,
                    "tool_calls": tool_calls
                }
            }]
        })
    }

    /// Helper to build a plain text response.
    fn text_response(text: &str) -> Value {
        json!({
            "choices": [{
                "message": {
                    "content": text
                }
            }]
        })
    }

    // ---- Tests ----

    #[test]
    fn test_parse_function_call_response_with_tools() {
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
        assert_eq!(calls[0].id, "call_abc123");
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
    fn test_parse_function_call_response_empty_tool_calls_array() {
        let response = json!({
            "choices": [{
                "message": {
                    "content": "Just text",
                    "tool_calls": []
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

    /// LLM returns tool_calls on the first call, then text on the second.
    /// Expects exactly 2 LLM calls.
    #[tokio::test]
    async fn test_agentic_loop_single_tool_call() {
        let mock = Arc::new(MockLlmProvider::new(vec![
            tool_calls_response(vec![("call_1", "list_entities", "{}")]),
            text_response("Here are your entities."),
        ]));
        let provider: Box<dyn LlmProvider> = Box::new(MockLlmProviderWrapper(Arc::clone(&mock)));
        let orchestrator = Orchestrator::new(provider, make_registry(), "test-model".to_string(), None);
        let mut state = ConversationState::new(1);

        let result = orchestrator.process_message("list my entities", None, &mut state).await.unwrap();

        assert_eq!(result, "Here are your entities.");
        assert_eq!(mock.get_call_count(), 2);
        // State should have: user + tool result + assistant
        assert_eq!(state.messages.len(), 3);
        assert_eq!(state.messages[0].role, "user");
        assert_eq!(state.messages[1].role, "tool");
        assert_eq!(state.messages[2].role, "assistant");
    }

    /// LLM chains two tool calls: list_entities, then turn_off_light, then text.
    /// Expects exactly 3 LLM calls.
    #[tokio::test]
    async fn test_agentic_loop_chained_calls() {
        let mock = Arc::new(MockLlmProvider::new(vec![
            tool_calls_response(vec![("call_1", "list_entities", "{}")]),
            tool_calls_response(vec![("call_2", "turn_off_light", "{\"entity_id\": \"light.kitchen\"}")]),
            text_response("Done! Kitchen light is off."),
        ]));
        let provider: Box<dyn LlmProvider> = Box::new(MockLlmProviderWrapper(Arc::clone(&mock)));
        let orchestrator = Orchestrator::new(provider, make_registry(), "test-model".to_string(), None);
        let mut state = ConversationState::new(1);

        let result = orchestrator.process_message("turn off kitchen light", None, &mut state).await.unwrap();

        assert_eq!(result, "Done! Kitchen light is off.");
        assert_eq!(mock.get_call_count(), 3);
        // State: user + tool1 + tool2 + assistant = 4
        assert_eq!(state.messages.len(), 4);
    }

    /// LLM always returns tool_calls — should hit MAX_TOOL_ITERATIONS and then
    /// force a text response on the final call.
    #[tokio::test]
    async fn test_agentic_loop_max_iterations() {
        // 5 tool_call responses (one per iteration) + 1 forced text response
        let mut responses = Vec::new();
        for i in 0..MAX_TOOL_ITERATIONS {
            responses.push(tool_calls_response(vec![
                (&format!("call_{}", i), "list_entities", "{}"),
            ]));
        }
        responses.push(text_response("Forced final answer."));

        // We need owned strings for the IDs since format! returns String
        let mock = Arc::new(MockLlmProvider::new(responses));
        let provider: Box<dyn LlmProvider> = Box::new(MockLlmProviderWrapper(Arc::clone(&mock)));
        let orchestrator = Orchestrator::new(provider, make_registry(), "test-model".to_string(), None);
        let mut state = ConversationState::new(1);

        let result = orchestrator.process_message("infinite tools", None, &mut state).await.unwrap();

        assert_eq!(result, "Forced final answer.");
        // MAX_TOOL_ITERATIONS loop calls + 1 forced final call
        assert_eq!(mock.get_call_count(), MAX_TOOL_ITERATIONS + 1);
    }

    /// LLM returns text directly without any tool calls.
    /// Expects exactly 1 LLM call.
    #[tokio::test]
    async fn test_agentic_loop_no_tools() {
        let mock = Arc::new(MockLlmProvider::new(vec![
            text_response("Hello! How can I help?"),
        ]));
        let provider: Box<dyn LlmProvider> = Box::new(MockLlmProviderWrapper(Arc::clone(&mock)));
        let orchestrator = Orchestrator::new(provider, make_registry(), "test-model".to_string(), None);
        let mut state = ConversationState::new(1);

        let result = orchestrator.process_message("hello", None, &mut state).await.unwrap();

        assert_eq!(result, "Hello! How can I help?");
        assert_eq!(mock.get_call_count(), 1);
        // State: user + assistant = 2
        assert_eq!(state.messages.len(), 2);
        assert_eq!(state.messages[0].role, "user");
        assert_eq!(state.messages[1].role, "assistant");
    }

    /// Sender name is stored on the user message in state.
    #[tokio::test]
    async fn test_sender_name_stored_in_state() {
        let mock = Arc::new(MockLlmProvider::new(vec![
            text_response("Hi there."),
        ]));
        let provider: Box<dyn LlmProvider> = Box::new(MockLlmProviderWrapper(Arc::clone(&mock)));
        let orchestrator = Orchestrator::new(provider, make_registry(), "test-model".to_string(), None);
        let mut state = ConversationState::new(1);

        orchestrator.process_message("hello", Some("Alice"), &mut state).await.unwrap();

        assert_eq!(state.messages[0].sender_name, Some("Alice".to_string()));
    }

    /// Without a sender name, the message is stored normally.
    #[tokio::test]
    async fn test_no_sender_name_is_none() {
        let mock = Arc::new(MockLlmProvider::new(vec![
            text_response("Hi."),
        ]));
        let provider: Box<dyn LlmProvider> = Box::new(MockLlmProviderWrapper(Arc::clone(&mock)));
        let orchestrator = Orchestrator::new(provider, make_registry(), "test-model".to_string(), None);
        let mut state = ConversationState::new(1);

        orchestrator.process_message("hello", None, &mut state).await.unwrap();

        assert_eq!(state.messages[0].sender_name, None);
    }

    // ---- Wrapper to share Arc<MockLlmProvider> as Box<dyn LlmProvider> ----

    /// Thin wrapper so we can share the mock via Arc while still boxing it
    /// as `Box<dyn LlmProvider>`. We need Arc to read `call_count` after the test.
    struct MockLlmProviderWrapper(Arc<MockLlmProvider>);

    #[async_trait::async_trait]
    impl LlmProvider for MockLlmProviderWrapper {
        async fn call_llm(&self, messages: Vec<Value>, tools: Vec<Value>, model: &str) -> Result<Value> {
            self.0.call_llm(messages, tools, model).await
        }
    }
}
