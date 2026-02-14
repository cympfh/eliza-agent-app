use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

const GROK_API_URL: &str = "https://api.x.ai/v1/chat/completions";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    stream: bool,
    temperature: f32,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Debug)]
pub enum GrokError {
    NetworkError(String),
    ApiError(String),
    ParseError(String),
}

impl std::fmt::Display for GrokError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GrokError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            GrokError::ApiError(msg) => write!(f, "API error: {}", msg),
            GrokError::ParseError(msg) => write!(f, "Parse error: {}", msg),
        }
    }
}

impl std::error::Error for GrokError {}

pub struct GrokClient {
    api_key: String,
    model: String,
    conversation_history: VecDeque<Message>,
    max_history_length: usize,
    system_prompt: String,
}

impl GrokClient {
    pub fn new(api_key: String, model: String, max_history_length: usize, system_prompt: String) -> Self {
        Self {
            api_key,
            model,
            conversation_history: VecDeque::new(),
            max_history_length,
            system_prompt,
        }
    }

    /// Send a message to Grok and get a response
    pub fn send_message(&mut self, user_message: &str) -> Result<String, GrokError> {
        // Add user message to history
        self.add_message("user".to_string(), user_message.to_string());

        println!("Sending message to Grok: {}", user_message);
        println!(
            "Current history length: {}",
            self.conversation_history.len()
        );

        // Prepare messages with system prompt at the beginning
        let mut messages: Vec<Message> = vec![Message {
            role: "system".to_string(),
            content: self.system_prompt.clone(),
        }];
        messages.extend(self.conversation_history.iter().cloned());

        // Prepare request
        let request = ChatRequest {
            model: self.model.clone(),
            messages,
            stream: false,
            temperature: 0.0,
        };

        // Send request
        let client = reqwest::blocking::Client::new();
        let response = client
            .post(GROK_API_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .map_err(|e| GrokError::NetworkError(format!("Failed to send request: {}", e)))?;

        let status = response.status();
        let response_text = response
            .text()
            .map_err(|e| GrokError::NetworkError(format!("Failed to read response: {}", e)))?;

        if !status.is_success() {
            return Err(GrokError::ApiError(format!(
                "API returned status {}: {}",
                status, response_text
            )));
        }

        // Parse response
        let chat_response: ChatResponse = serde_json::from_str(&response_text).map_err(|e| {
            GrokError::ParseError(format!(
                "Failed to parse response: {}. Response was: {}",
                e, response_text
            ))
        })?;

        let assistant_message = chat_response
            .choices
            .first()
            .ok_or_else(|| GrokError::ParseError("No choices in response".to_string()))?
            .message
            .content
            .clone();

        // Add assistant message to history
        self.add_message("assistant".to_string(), assistant_message.clone());

        println!("Grok response: {}", assistant_message);
        Ok(assistant_message)
    }

    /// Add a message to conversation history and maintain max length
    fn add_message(&mut self, role: String, content: String) {
        self.conversation_history.push_back(Message { role, content });

        // Keep only the last max_history_length messages
        while self.conversation_history.len() > self.max_history_length {
            self.conversation_history.pop_front();
        }
    }

    /// Clear conversation history
    pub fn clear_history(&mut self) {
        self.conversation_history.clear();
        println!("Conversation history cleared");
    }

    /// Get current conversation history
    pub fn get_history(&self) -> &VecDeque<Message> {
        &self.conversation_history
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_management() {
        let mut client = GrokClient::new(
            "test_key".to_string(),
            "grok-beta".to_string(),
            3,
            "Test system prompt".to_string()
        );

        client.add_message("user".to_string(), "Message 1".to_string());
        assert_eq!(client.conversation_history.len(), 1);

        client.add_message("assistant".to_string(), "Response 1".to_string());
        assert_eq!(client.conversation_history.len(), 2);

        client.add_message("user".to_string(), "Message 2".to_string());
        assert_eq!(client.conversation_history.len(), 3);

        // Should remove oldest message
        client.add_message("assistant".to_string(), "Response 2".to_string());
        assert_eq!(client.conversation_history.len(), 3);
        assert_eq!(client.conversation_history[0].content, "Response 1");
    }

    #[test]
    fn test_clear_history() {
        let mut client = GrokClient::new(
            "test_key".to_string(),
            "grok-beta".to_string(),
            5,
            "Test system prompt".to_string()
        );
        client.add_message("user".to_string(), "Test".to_string());
        assert_eq!(client.conversation_history.len(), 1);

        client.clear_history();
        assert_eq!(client.conversation_history.len(), 0);
    }
}
