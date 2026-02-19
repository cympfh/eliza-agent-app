use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

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
    message: Message,
}

#[derive(Debug)]
pub enum ElizaError {
    NetworkError(String),
    ApiError(String),
    ParseError(String),
}

impl std::fmt::Display for ElizaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ElizaError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            ElizaError::ApiError(msg) => write!(f, "API error: {}", msg),
            ElizaError::ParseError(msg) => write!(f, "Parse error: {}", msg),
        }
    }
}

impl std::error::Error for ElizaError {}

pub struct ElizaClient {
    server_url: String,
    model: String,
    conversation_history: VecDeque<Message>,
    max_history_length: usize,
    system_prompt: String,
}

impl ElizaClient {
    pub fn new(server_url: String, model: String, max_history_length: usize, system_prompt: String) -> Self {
        Self {
            server_url,
            model,
            conversation_history: VecDeque::new(),
            max_history_length,
            system_prompt,
        }
    }

    /// Send a message to Eliza and get a response
    pub fn send_message(&mut self, user_message: &str) -> Result<String, ElizaError> {
        // Add user message to history
        self.add_message("user".to_string(), user_message.to_string());

        println!("Sending message to Eliza: {}", user_message);
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

        // Build the full URL (server_url + /chat if not already included)
        let url = if self.server_url.ends_with("/chat") {
            self.server_url.clone()
        } else {
            format!("{}/chat", self.server_url)
        };

        // Send request
        let client = reqwest::blocking::Client::new();
        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .map_err(|e| ElizaError::NetworkError(format!("Failed to send request: {}", e)))?;

        let status = response.status();
        let response_text = response
            .text()
            .map_err(|e| ElizaError::NetworkError(format!("Failed to read response: {}", e)))?;

        if !status.is_success() {
            return Err(ElizaError::ApiError(format!(
                "API returned status {}: {}",
                status, response_text
            )));
        }

        // Parse response
        let chat_response: ChatResponse = serde_json::from_str(&response_text).map_err(|e| {
            ElizaError::ParseError(format!(
                "Failed to parse response: {}. Response was: {}",
                e, response_text
            ))
        })?;

        let assistant_message = chat_response.message.content.clone();

        // Add assistant message to history
        self.add_message("assistant".to_string(), assistant_message.clone());

        println!("Eliza response: {}", assistant_message);
        Ok(assistant_message)
    }

    /// Add a message to conversation history and maintain max length
    fn add_message(&mut self, role: String, content: String) {
        self.conversation_history.push_back(Message { role, content });

        // Save memory and compact history if it exceeds max length
        if self.conversation_history.len() > self.max_history_length {
            const COMPACT_SIZE: usize = 5;
            if let Err(e) = self.save_memory() {
                eprintln!("Failed to save memory (max length reached): {}", e);
            }
            while self.conversation_history.len() > COMPACT_SIZE {
                self.conversation_history.pop_front();
            }
        }
    }

    /// Save conversation history to /memory endpoint
    pub fn save_memory(&self) -> Result<(), ElizaError> {
        if self.conversation_history.is_empty() {
            return Ok(());
        }

        let mut messages: Vec<Message> = vec![Message {
            role: "system".to_string(),
            content: self.system_prompt.clone(),
        }];
        messages.extend(self.conversation_history.iter().cloned());

        let request = ChatRequest {
            model: self.model.clone(),
            messages,
            stream: false,
            temperature: 0.0,
        };

        let url = format!(
            "{}/memory",
            self.server_url.trim_end_matches("/chat")
        );

        println!("Saving memory to: {}", url);
        let client = reqwest::blocking::Client::new();
        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .map_err(|e| ElizaError::NetworkError(format!("Failed to save memory: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(ElizaError::ApiError(format!(
                "Memory API returned status {}: {}",
                status, body
            )));
        }

        println!("Memory saved successfully");
        Ok(())
    }

    /// Clear conversation history
    pub fn clear_history(&mut self) {
        self.conversation_history.clear();
        println!("Conversation history cleared");
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_management() {
        let mut client = ElizaClient::new(
            "http://localhost:9095".to_string(),
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
        let mut client = ElizaClient::new(
            "http://localhost:9095".to_string(),
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
