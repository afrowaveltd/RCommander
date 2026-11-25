# MCP agents

RCommander speaks the Model Context Protocol (MCP) in both directions.

## The app as an MCP server

Start `rcommander mcp` as a separate stdio server process. External agents can
then drive the file manager: list panels and directories, navigate, mark files, copy,
move, delete, create directories, rename, read and write text files, search, ask
for disk space and read this help.

**Server transport currently implemented:** stdio only. A client launches
`rcommander mcp` and communicates over standard input/output. There is **no HTTP
server entry point**; asking for one (`--http`, `--token`) fails with an explicit
message instead of being ignored. HTTP *client* connections below are a different
feature.

### Approving a destructive tool

Tools that can destroy data (delete, move, overwrite, writing files, and the Git
tools `git_commit`/`git_checkout`/`git_push`) are gated. The gate is chosen on the
command line:

| Command | What happens |
| --- | --- |
| `rcommander mcp` | asks **on your terminal** before such a tool runs; without a terminal (a build agent, a container) every destructive tool is refused |
| `rcommander mcp --yes` | no question; the tool runs. The capability policy and the audit log still apply |
| `rcommander mcp --no-ask` | never asks and never allows — a read-only agent |

Two details make the answer trustworthy. First, the question goes to the
**terminal**, not to standard output: that stream carries the JSON-RPC protocol,
so a question written there would corrupt it — and reading the answer from
standard input would swallow a message. Second, anything that is not an explicit
`y`/`yes`/`ano` is a **no**, so a stray newline or a closed terminal can never
approve a deletion. The default is refusal, and a missing terminal is not a yes.

An approval is only one gate. The capability policy (settings `[agent] profile`,
`require_approval`, grants) is checked first, and every decision is written to the
audit log with the hash chain described in the *Configuration* topic.

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
