@CLAUDE

See README.md and below

## Architecture

```
Windows mic -> [Text-to-Speech] -> AI Agent -> [VRChat (via OSC)]
```

### References

mic to Text-to-Speech: ~/git/winh/src/ (this is written in Rust as native application for Windows11)

VRChat via OSC: ~/bin/vrchatbox (this is written in Python)
this is windows native application, so OSC is sent to localhost.

### UI

**Settings**

OPENAI_API_KEY for Text-to-Speech
max_length_of_conversation_history (for AI Agent, default=5)

Note: XAI_API_KEY is configured on the server side (eliza-agent-server), not in this client.

**Start to talk with AI Agent**
websocket connection to AI Agent is established.

**Stop talking with AI Agent**
websocket connection to AI Agent is closed.

**model for Text-to-Speech**

**model for AI Agent**

## logging

セッションごとにログファイルを作成して、会話の履歴やエラー情報を記録する。

~/.eliza-agent/logs/session_YYYYMMDD_HHMMSS.log

```
{"type": "conversation", "timestamp": "2024-06-01T12:00:00Z", "message": "Hello", "source": "user"}
{"type": "conversation", "timestamp": "2024-06-01T12:00:05Z", "message": "Hi there! How can I assist you today?", "source": "eliza"}
{"type": "error", "timestamp": "2024-06-01T12:01:00Z", "message": "connection lost"}
```


