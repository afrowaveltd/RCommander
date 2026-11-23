# MCP agents

RCommander speaks the Model Context Protocol (MCP) in both directions.

## The app as an MCP server

Enable *Server* in *Configuration ▸ Options ▸ Agent*. External agents can then
drive the file manager: list panels and directories, navigate, mark files, copy,
move, delete, create directories, rename, read and write text files, search, ask
for disk space and read this help.

Two transports are available:

- **stdio** — the agent starts `rcommander mcp` and talks JSON-RPC over standard
  input and output. This is what most MCP clients expect.
- **HTTP** — a local endpoint for clients that cannot spawn processes. Set a
  token to require authentication.

Risky tools (delete, move, overwrite, writing files) can require confirmation. In
that case the server answers with an error explaining that the user must approve
the action, instead of executing it silently.

## The app as an MCP client

The agent panel connects to any MCP server — local command or HTTP endpoint — and
lets you run its tools from inside RCommander. Configure servers under
`[agent] clients` in the settings file:

```toml
[[agent.clients]]
name = "filesystem"
transport = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/home"]
```

## Safety

The server only exposes what the protocol advertises; nothing runs unless a tool
is called. Every call is shown in the status line, and the confirmation policy is
yours to configure. Treat the HTTP transport like any other local service: bind
it to localhost and use a token.
