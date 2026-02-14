use rosc::encoder;
use rosc::{OscMessage, OscPacket, OscType};
use std::net::UdpSocket;

#[derive(Debug)]
pub enum VRChatError {
    SocketError(String),
    SendError(String),
}

impl std::fmt::Display for VRChatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VRChatError::SocketError(msg) => write!(f, "Socket error: {}", msg),
            VRChatError::SendError(msg) => write!(f, "Send error: {}", msg),
        }
    }
}

impl std::error::Error for VRChatError {}

pub struct VRChatClient {
    pub target_addr: String,
}

impl VRChatClient {
    pub fn new() -> Self {
        Self {
            target_addr: "127.0.0.1:9000".to_string(), // VRChat OSC default port
        }
    }

    /// Send a message to VRChat via OSC
    pub fn send_message(&self, message: &str) -> Result<(), VRChatError> {
        println!("[VRChat OSC] Preparing to send message");
        println!("[VRChat OSC] Target: {}", self.target_addr);
        println!("[VRChat OSC] Message length: {} bytes", message.len());

        // Create UDP socket
        println!("[VRChat OSC] Binding UDP socket...");
        let socket = UdpSocket::bind("0.0.0.0:0")
            .map_err(|e| VRChatError::SocketError(format!("Failed to bind socket: {}", e)))?;

        let local_addr = socket.local_addr()
            .map(|a| a.to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        println!("[VRChat OSC] Socket bound to: {}", local_addr);

        // Send the message
        println!("[VRChat OSC] Encoding and sending OSC message...");
        self.send_chatbox_input(&socket, message, true)?;

        println!("[VRChat OSC] ✓ Message sent successfully");
        Ok(())
    }

    /// Send chatbox input message
    fn send_chatbox_input(
        &self,
        socket: &UdpSocket,
        text: &str,
        notify: bool,
    ) -> Result<(), VRChatError> {
        let msg = OscMessage {
            addr: "/chatbox/input".to_string(),
            args: vec![
                OscType::String(text.to_string()),
                OscType::Bool(true),   // immediate
                OscType::Bool(notify), // notify sound
            ],
        };

        self.send_osc_message(socket, msg)
    }

    /// Send an OSC message
    fn send_osc_message(&self, socket: &UdpSocket, msg: OscMessage) -> Result<(), VRChatError> {
        println!("[VRChat OSC] Encoding OSC packet for address: {}", msg.addr);
        let packet = OscPacket::Message(msg);
        let msg_buf = encoder::encode(&packet)
            .map_err(|e| VRChatError::SendError(format!("Failed to encode OSC message: {}", e)))?;

        println!("[VRChat OSC] Encoded {} bytes, sending to {}", msg_buf.len(), self.target_addr);
        let bytes_sent = socket
            .send_to(&msg_buf, &self.target_addr)
            .map_err(|e| {
                VRChatError::SendError(format!("Failed to send OSC message: {}", e))
            })?;

        println!("[VRChat OSC] Sent {} bytes via UDP", bytes_sent);
        Ok(())
    }
}

impl Default for VRChatClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = VRChatClient::new();
        assert_eq!(client.target_addr, "127.0.0.1:9000");
    }
}
