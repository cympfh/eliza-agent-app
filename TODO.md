@CLAUDE

See README.md and below

## Architecture

```
Windows mic -> [Text-to-Speech] -> Grok -> [VRChat (via OSC)]
```

### References

mic to Text-to-Speech: ~/git/winh/src/ (this is written in Rust as native application for Windows11)

VRChat via OSC: ~/bin/vrchatbox (this is written in Python)
this is windows native application, so OSC is sent to localhost.

### UI

**Settings**

OPENAI_API_KEY for Text-to-Speech
XAI_API_KEY for Grok
max_length_of_conversation_history (for Grok, default=5)

**Start to talk with Grok**
websocket connection to Grok is established.

**Stop talking with Grok**
websocket connection to Grok is closed.

