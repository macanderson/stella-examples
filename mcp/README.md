# MCP servers

Stella speaks the Model Context Protocol. Servers are configured per
workspace in **`<repo>/.stella/mcp.toml`** — one `[servers.<name>]` table
per server. Reference:
[Agent tools → MCP](https://stella.oxagen.sh/docs/agent-tools/mcp).

**Two transports**

- `transport = "stdio"` — Stella spawns the server as a child process.
  **The child inherits no ambient environment** — only the keys you list
  under `env` are passed through. Deliberate: an MCP server never sees your
  API keys by accident.
- `transport = "http"` — Stella connects to a running server over HTTP;
  `headers` carries auth (values are redacted from debug output).

[`mcp.toml`](mcp.toml) in this directory shows both.

**Managing servers**

```bash
stella mcp list                # what's configured + connectivity
stella mcp search <query>      # search the MCP registry
stella mcp install <name>      # add a server from the registry
stella mcp login <name>        # OAuth flow → .stella/mcp_oauth.json
stella mcp usage               # per-server call/token usage
```

The registry endpoint itself is configurable in `settings.json`:

```json
{ "mcp": { "registry_url": "https://registry.example.com" } }
```

> **Trust boundary:** like project hooks, MCP servers declared in a cloned
> repo need `STELLA_TRUST_PROJECT=1` before Stella will spawn them.
