use crate::error::Result;
use serde_json::Value;

pub struct LMStudioProvider {
    #[allow(dead_code)]
    url: String,
}

impl LMStudioProvider {
    pub fn new(url: String) -> Self {
        Self { url }
    }

    pub async fn call_llm(&self, _prompt: String, _tools: Vec<Value>) -> Result<Value> {
        // TODO: implement
        Ok(Value::Null)
    }
}
