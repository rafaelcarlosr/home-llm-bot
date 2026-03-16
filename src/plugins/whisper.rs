use crate::error::{BotError, Result};
use reqwest::{Client, multipart};
use serde_json::Value;

/// Whisper speech-to-text provider (OpenAI-compatible API).
///
/// **Rust learning note on `Vec<u8>`:**
/// This is Rust's owned byte vector (heap-allocated, growable).
/// Similar to Java's `byte[]` but with ownership semantics.
/// When this value moves into `transcribe()`, ownership transfers to the function.
pub struct WhisperProvider {
    url: String,
    client: Client,
}

impl WhisperProvider {
    pub fn new(url: String) -> Self {
        Self {
            url,
            client: Client::new(),
        }
    }

    /// Transcribe audio bytes to text using Whisper.
    ///
    /// **Rust learning note on multipart:**
    /// Builder pattern with `Form::new().part(...).text(...)` is common in Rust.
    /// Each method consumes `self` and returns `Self`, making the chain type-safe.
    /// In Java: `new FormBuilder().addPart(...).addText(...).build()`
    pub async fn transcribe(&self, audio_data: Vec<u8>) -> Result<String> {
        // Build multipart form with audio file and model
        let audio_part = multipart::Part::bytes(audio_data)
            .file_name("audio.ogg")
            .mime_str("audio/ogg")?; // Telegram voice messages are OGG/Opus codec

        let form = multipart::Form::new()
            .part("file", audio_part)
            .text("model", "whisper-1"); // Model name expected by OpenAI-compatible API

        let response = self.client
            .post(format!("{}/v1/audio/transcriptions", self.url))
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(BotError::Whisper(format!("HTTP {}: {}", status, body)));
        }

        let json: Value = response.json().await?;

        // Extract "text" field from response
        // **Rust learning note on Option pattern:**
        // `.as_str()` returns `Option<&str>`. Chaining with `.map()` and `.ok_or_else()`
        // is equivalent to Java's `Optional.map().orElseThrow()`.
        json["text"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| BotError::Whisper("Missing 'text' field in response".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_transcribe_connection_refused() {
        let provider = WhisperProvider::new("http://localhost:19999".into()); // Non-existent port
        let result = provider.transcribe(vec![0xFF, 0xFB]).await;
        assert!(result.is_err()); // Should fail with connection error
    }

    #[tokio::test]
    #[ignore] // Requires real Whisper instance
    async fn test_real_transcription() {
        // Uncomment to test against real Whisper:
        // let provider = WhisperProvider::new("http://localhost:9000".into());
        // let fake_audio = vec![0xFF, 0xFB]; // Dummy bytes
        // let result = provider.transcribe(fake_audio).await;
        // assert!(result.is_ok());
    }
}
