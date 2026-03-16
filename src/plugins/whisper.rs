use crate::error::Result;

pub struct WhisperProvider {
    #[allow(dead_code)]
    url: String,
}

impl WhisperProvider {
    pub fn new(url: String) -> Self {
        Self { url }
    }

    pub async fn transcribe(&self, _audio_data: Vec<u8>) -> Result<String> {
        // TODO: implement
        Ok("transcribed text".to_string())
    }
}
