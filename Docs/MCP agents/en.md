# MCP agents

RCommander speaks the Model Context Protocol (MCP) in both directions.

## What MCP is, in one page

MCP (Model Context Protocol) is a JSON-RPC protocol for *applications offering capabilities to
programs*. It has three kinds of capability, and knowing which is which explains most of what a
connected app can do:

| Kind | What it is | Example |
| --- | --- | --- |
| **tools** | actions, each with a JSON schema for its arguments | `create_icon(prompt, size)` from a drawing app |
| **resources** | readable items, addressed by a URL-like id | a document, an image, a database row |
| **prompts** | templates the application publishes | "summarise this meeting" for a mail app |

A client connects, asks what a server publishes (`tools/list`, `resources/list`, `prompts/list`),
then calls what the human allowed. RCommander is **both**: `rcommander mcp` is a server other agents
can drive, and the agent panel is a client that connects to other servers.

The important thing it does *not* mean: a tool is not a permission. An application offering
`delete_everything` does not make it safe or allowed; what a model may call is RCommander's business,
and it goes through the same policy and approval dialog as everything else.

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
| `rcommander mcp` | asks the **running RCommander window** (a dialog with the three answers), or **on your terminal** when no window is running; without either, every destructive tool is refused |
| `rcommander mcp --yes` | no question; the tool runs. The capability policy and the audit log still apply |
| `rcommander mcp --no-ask` | never asks and never allows — a read-only agent |

When the window is running, the dialog is the same one an in-process agent gets,
with **allow now**, **allow for this session** and the red **allow always** — and
it says that an agent is waiting, because a blocked client is easy to overlook.
The answer travels back over a loopback socket whose address and per-run token
live in an owner-only file in your runtime directory, so only your own processes
can ask. Nothing is decided by the socket: the human answers, the window forwards
the answer.

Two details make the terminal answer trustworthy. First, the question goes to the
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

*Net → MCP agents* opens the panel. Each entry of `[[agent.clients]]` in
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
