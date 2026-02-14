# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**talk-with-grok** - A Rust native application for Windows 11 that enables voice conversations with Grok AI in VRChat.

## Architecture

The application follows this data flow:
```
Windows mic -> [Speech-to-Text] -> Grok API -> [VRChat via OSC]
```

### Key Components

- **Speech-to-Text**: Uses OpenAI API for converting voice to text
- **Grok Integration**: WebSocket connection to Grok (xAI API) for conversational AI
- **VRChat Output**: Sends responses via OSC to localhost using `~/bin/vrchatbox` (Python script)

### External Dependencies

- `~/git/winh/src/`: Windows native Rust code for microphone input handling
- `~/bin/vrchatbox`: Python script for VRChat OSC communication (localhost)

## Configuration

The application requires:
- `OPENAI_API_KEY`: For speech-to-text functionality
- `XAI_API_KEY`: For Grok API access
- `max_length_of_conversation_history`: Conversation history limit (default: 5)

## Build & Run Commands

```bash
# Build the project
cargo build

# Run the application
cargo run

# Build release version
cargo build --release

# Run tests
cargo test
```

## Development Notes

- This is a Windows 11 native application
- OSC communication targets localhost (same machine running VRChat)
- WebSocket connection lifecycle: established on "Start", closed on "Stop"
- Edition specified as "2024" in Cargo.toml (note: this may need adjustment to a valid Rust edition)
