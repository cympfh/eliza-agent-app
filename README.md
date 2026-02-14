# talk with Grok

- Rust native application for Windows11
- You can talk with Grok by voice, in VRChat

## Requirements

### Grok HTTP Server

This application requires a local HTTP server to communicate with Grok API. The server is available at:

**https://github.com/cympfh/grok-http-server**

The server provides:
- Stateless chat API endpoint
- Built-in x_search (X/Twitter search) and web_search tools
- Default port: 9096 (configurable)

#### Starting the server:

```bash
# Clone and setup
git clone https://github.com/cympfh/grok-http-server
cd grok-http-server
uv sync

# Set API key
export XAI_API_KEY="your-api-key-here"

# Start server (default port: 8000, but you can specify 9096)
python server.py --port 9096
```

#### Configuration

The server URL can be configured in the application's Settings:
- Default: `http://localhost:9096`
- Customizable to any host/port

