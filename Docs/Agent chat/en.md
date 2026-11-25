# Agent chat

The chat is a conversation with a language model *inside* the file manager, so you
can ask about what you are looking at without switching windows. Open it from
**Configuration → Agent chat** or with the `cm_AiChat` command.

## What it does

- The answer is **streamed**: text appears while the model is still generating.
  *Stop* keeps what already arrived instead of throwing it away.
- The transcript is the request history, so a follow-up question ("and the second
  one?") sees everything said before it.
- *New conversation* clears the transcript and starts from scratch.
- `Enter` sends, `Shift+Enter` adds a line — so multi-line questions work.

## Models

The chat uses the model selected in the model picker (see the *Configuration*
topic). RCommander ships with **Ollama** support, so a local model is the default
and nothing leaves the machine. Provider, address and model live in the settings
file:

```toml
[agent]
provider_kind = "ollama"
provider_url = "http://127.0.0.1:11434"
model = "qwen3:8b"
```

When no model is selected the chat says so instead of failing silently; open the
picker, press *Look for models again* and pick one.

## What the chat cannot do

The conversation has **no access to files**. It cannot read, write, copy or
delete anything. That restriction is deliberate: every action with side effects
belongs to the privilege policy, which asks for permission and writes an audit
entry. Tools that do change things are the MCP agent tools, and they are guarded
by the same policy — a chat answer can suggest an action, but performing it
always goes through that gate.

Stopping the model and closing the window are always safe: an unfinished answer
only loses the rest of the text, never data.

## Known limits

- The chat does not run tools by itself; tool calling needs a model that supports
  it and is not wired into this window yet.
- Answers are rendered as Markdown — headings, lists, tables, quotes, inline code
  and fenced code blocks — but code is **not** syntax-highlighted yet, and links
  in an answer are drawn as text rather than opened.
- The transcript is kept in memory only. Closing RCommander ends the
  conversation; a local store is planned.
- A half-received answer is rendered as it arrives, so a Markdown table or fence
  can look odd for a moment and settles when the rest of the line arrives.
