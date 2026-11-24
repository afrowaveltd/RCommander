# MCP agents

RCommander speaks the Model Context Protocol (MCP) in both directions.

## The app as an MCP server

Start `rcommander mcp` as a separate stdio server process. External agents can
then drive the file manager: list panels and directories, navigate, mark files, copy,
move, delete, create directories, rename, read and write text files, search, ask
for disk space and read this help.

**Server transport currently implemented:** stdio only. A client launches
`rcommander mcp` and communicates over standard input/output. The CLI help
mentions an HTTP mode, but `run_mcp` currently ignores these flags; **there is
no working HTTP server entry point**. HTTP *client* connections below are a
different feature.

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
yours to configure. When connecting the **client** to a remote HTTP MCP server, choose a trusted,
authenticated endpoint and avoid leaking credentials. Do not assume that the
unimplemented RCommander HTTP **server** provides protection.

## The agent panel

*Net ▸ MCP agents* opens the panel. Each entry of `[[agent.clients]]` in
`settings.toml` becomes a button; pressing it connects to that server, lists the
tools it publishes and lets you pick one, edit its JSON arguments and run it. The
result (or the error the server returned) is shown below the tool selector.

```toml
[[agent.clients]]
name = "filesystem"
transport = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/home"]
```

A transport is `stdio` (a command is started and spoken to over stdin/stdout) or
`http` (a JSON-RPC endpoint, `url` is required). Entries that lack the required
field are ignored, so a typo cannot break the panel. Servers that are slow or
unreachable report a timeout instead of blocking the window.
