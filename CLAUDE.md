# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**talk-with-grok** - A Rust native application for Windows 11 that enables voice conversations with Grok AI in VRChat.

## Current Implementation Status

**Status: Implementing Option 1 (VAD-based approach with HTTP API)**

We are currently implementing a VAD (Voice Activity Detection) approach that continuously monitors microphone input and automatically triggers speech recognition when sound is detected.

## Architecture

### Chosen Approach: Option 1 - VAD with HTTP API

```
[Continuous Mic Monitoring]
    ↓ Voice detected (amplitude > threshold)
[Auto Recording Start] (buffer accumulation)
    ↓ Silence detected (2 seconds)
[Auto Recording Stop]
    ↓
[Speech-to-Text (OpenAI Whisper)]
    ↓
[Grok API (HTTP POST)]
    ↓
[VRChat OSC Output via vrchatbox]
    ↓
[Return to Monitoring State]
```

### Implementation Details

**State Machine:**
```rust
enum AppState {
    Idle,        // Stopped
    Monitoring,  // Waiting for voice (monitoring volume)
    Recording,   // Recording audio
    Processing,  // Transcribing and sending to Grok
}
```

**Key Features:**
- Continuous microphone monitoring without manual button presses
- Automatic recording start when amplitude exceeds `start_threshold`
- Automatic recording stop after `silence_duration_secs` of silence
- Based on winh codebase (reuses audio.rs, openai.rs)

**Configuration Parameters:**
```rust
start_threshold: f32       // Threshold to start recording (e.g., 0.02)
silence_threshold: f32     // Threshold to detect silence (e.g., 0.01)
silence_duration_secs: f32 // Duration of silence to stop (e.g., 2.0)
```

### Alternative Approach: Option 2 (Not Currently Implemented)

An alternative approach using Grok's real-time audio API (WebSocket streaming) was considered but **not chosen** because:
- Requires continuous WebSocket audio streaming
- Official documentation is unreliable
- Lack of reference implementations
- Higher implementation risk

We can revisit this approach if Option 1 proves insufficient.

### Key Components

- **Speech-to-Text**: OpenAI Whisper API (via HTTP)
- **Grok Integration**: HTTP POST to Grok API (simple, reliable)
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
- Most code can be reused from `~/git/winh/src/` (see REFERENCE_NOTES.md for details)
- Edition specified as "2024" in Cargo.toml (note: this may need adjustment to a valid Rust edition)

## Implementation Steps

1. **Audio Module** (reuse from winh)
   - Copy and adapt `audio.rs` for continuous monitoring
   - Add state transitions: Monitoring → Recording → Processing → Monitoring

2. **Speech-to-Text Module** (reuse from winh)
   - Copy `openai.rs` for Whisper API integration
   - No modifications needed

3. **Grok API Module** (new implementation)
   - Simple HTTP POST client
   - Conversation history management (max_length_of_conversation_history)
   - Error handling and retry logic

4. **VRChat Output** (external command)
   - Use `vrchatbox` via subprocess
   - Consider using `--lazy` option for typing animation

5. **UI** (adapt from winh)
   - Start/Stop button for Monitoring mode
   - Real-time status display (Idle/Monitoring/Recording/Processing)
   - Conversation history display
   - Settings panel for thresholds and API keys
