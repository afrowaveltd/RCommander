//! Shared test helpers: a real-filesystem [`FakeHost`] for the server tests and a
//! scripted peer for the client tests.
//!
//! Nothing here is part of the shipped crate; both integration test binaries
//! include this module with `mod common;`.
#![allow(dead_code)]

use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use rc_agent::{
    Confirmer, FileManagerHost, HostOpResult, HostPanel, LanguageHost, PanelSide, SearchRequest,
    SelectMode,
};
use rc_core::entry::{Attrs, Entry, EntryKind};
use rc_core::mask::{MaskSet, Pattern};
use serde_json::Value;

// ---------------------------------------------------------------------------
// FakeHost
// ---------------------------------------------------------------------------

/// A [`FileManagerHost`] backed by a real temporary directory.
///
/// The two panels start in the same directory (`left` in a sub-directory) so a
/// test can copy between them; every mutating call is logged in `log` so a test
/// can assert what the server asked for.
pub struct FakeHost {
    /// Keeps the temporary directory alive for as long as the host lives.
    pub dir: tempfile::TempDir,
    pub left: Mutex<PathBuf>,
    pub right: Mutex<PathBuf>,
    pub active: Mutex<PanelSide>,
    pub selection: Mutex<Vec<String>>,
    pub log: Mutex<Vec<String>>,
    pub view_mode: Mutex<String>,
    pub language: Mutex<String>,
    /// When set, `list_directory` panics — used to prove that a misbehaving GUI
    /// becomes a `-32603` error instead of taking the server down.
    pub panic_in_list_directory: AtomicBool,
}

impl std::fmt::Debug for FakeHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakeHost")
            .field("root", &self.dir.path())
            .finish_non_exhaustive()
    }
}

impl FakeHost {
    /// Build a temporary tree:
    ///
    /// ```text
    /// root/alpha.txt        "hello alpha"
    /// root/beta.md          "beta docs"
    /// root/.hidden.txt      "secret"
    /// root/big.txt          200 'x' characters
    /// root/sub/gamma.rs     "fn main() {}"
    /// root/sub/deep/delta.txt "needle inside delta"
    /// ```
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join("sub/deep")).expect("sub dirs");
        std::fs::write(root.join("alpha.txt"), "hello alpha").expect("alpha");
        std::fs::write(root.join("beta.md"), "beta docs").expect("beta");
        std::fs::write(root.join(".hidden.txt"), "secret").expect("hidden");
        std::fs::write(root.join("big.txt"), "x".repeat(200)).expect("big");
        std::fs::write(root.join("sub/gamma.rs"), "fn main() {}").expect("gamma");
        std::fs::write(root.join("sub/deep/delta.txt"), "needle inside delta").expect("delta");
        FakeHost {
            left: Mutex::new(root.join("sub")),
            right: Mutex::new(root.clone()),
            active: Mutex::new(PanelSide::Left),
            selection: Mutex::new(Vec::new()),
            log: Mutex::new(Vec::new()),
            view_mode: Mutex::new("brief".to_string()),
            language: Mutex::new("en".to_string()),
            panic_in_list_directory: AtomicBool::new(false),
            dir,
        }
    }

    /// Root of the temporary tree.
    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    /// Absolute path inside the temporary tree.
    pub fn path(&self, relative: &str) -> String {
        self.root().join(relative).to_string_lossy().to_string()
    }

    /// Current directory of one panel.
    pub fn panel(&self, side: PanelSide) -> PathBuf {
        match side {
            PanelSide::Left => self.left.lock().expect("left").clone(),
            PanelSide::Right => self.right.lock().expect("right").clone(),
        }
    }

    fn set_panel(&self, side: PanelSide, path: PathBuf) {
        match side {
            PanelSide::Left => *self.left.lock().expect("left") = path,
            PanelSide::Right => *self.right.lock().expect("right") = path,
        }
    }

    fn record(&self, entry: String) {
        self.log.lock().expect("log").push(entry);
    }

    /// Everything the server asked the host to do, in order.
    pub fn recorded(&self) -> Vec<String> {
        self.log.lock().expect("log").clone()
    }

    fn entries_of(&self, directory: &Path) -> Result<Vec<Entry>, String> {
        let read = std::fs::read_dir(directory)
            .map_err(|error| format!("cannot read `{}`: {error}", directory.display()))?;
        let mut entries = Vec::new();
        for item in read {
            let item = item.map_err(|error| format!("cannot read an entry: {error}"))?;
            let path = item.path();
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|error| format!("cannot stat `{}`: {error}", path.display()))?;
            let name = item.file_name().to_string_lossy().to_string();
            entries.push(entry_from_metadata(name, path, &metadata));
        }
        entries.sort_by_key(|entry| entry.name.to_lowercase());
        Ok(entries)
    }
}

/// Build an [`Entry`] from metadata, mirroring what the real listing does.
fn entry_from_metadata(name: String, path: PathBuf, metadata: &std::fs::Metadata) -> Entry {
    let kind = if metadata.file_type().is_symlink() {
        match std::fs::metadata(&path) {
            Ok(target) if target.is_dir() => EntryKind::SymlinkToDir,
            Ok(_) => EntryKind::SymlinkToFile,
            Err(_) => EntryKind::SymlinkBroken,
        }
    } else if metadata.is_dir() {
        EntryKind::Dir
    } else if metadata.is_file() {
        EntryKind::File
    } else {
        EntryKind::Special
    };
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|since| since.as_secs() as i64);
    let mut attrs = Attrs {
        readonly: metadata.permissions().readonly(),
        ..Attrs::default()
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        attrs.unix_mode = Some(metadata.mode() & 0o7777);
    }
    let link_target = if metadata.file_type().is_symlink() {
        std::fs::read_link(&path).ok()
    } else {
        None
    };
    Entry {
        name,
        path,
        kind,
        size: metadata.len(),
        mtime,
        ctime: None,
        attrs,
        link_target,
    }
}

impl FileManagerHost for FakeHost {
    fn panels(&self) -> Vec<HostPanel> {
        let active = *self.active.lock().expect("active");
        PanelSide::ALL
            .iter()
            .map(|side| {
                let path = self.panel(*side);
                let entries = self.entries_of(&path).unwrap_or_default();
                let selection = self.selection.lock().expect("selection").len();
                HostPanel {
                    side: *side,
                    path: path.to_string_lossy().to_string(),
                    active: *side == active,
                    view_mode: self.view_mode.lock().expect("view").clone(),
                    selected: if *side == active { selection } else { 0 },
                    entries: entries.len(),
                }
            })
            .collect()
    }

    fn active_panel(&self) -> HostPanel {
        let active = *self.active.lock().expect("active");
        self.panels()
            .into_iter()
            .find(|panel| panel.side == active)
            .expect("active panel exists")
    }

    fn list_directory(
        &self,
        panel: Option<PanelSide>,
        path: Option<&str>,
    ) -> Result<Vec<Entry>, String> {
        assert!(
            !self.panic_in_list_directory.load(Ordering::SeqCst),
            "fake host exploded"
        );
        let directory = match path {
            Some(path) => PathBuf::from(path),
            None => self.panel(panel.unwrap_or(*self.active.lock().expect("active"))),
        };
        if !directory.is_dir() {
            return Err(format!("`{}` is not a directory", directory.display()));
        }
        self.entries_of(&directory)
    }

    fn open_directory(&self, panel: PanelSide, path: &str) -> Result<(), String> {
        let target = PathBuf::from(path);
        if !target.is_dir() {
            return Err(format!("`{path}` is not a directory"));
        }
        self.set_panel(panel, target);
        self.record(format!("navigate {panel} {path}"));
        Ok(())
    }

    fn selection(&self, _panel: Option<PanelSide>) -> Result<Vec<String>, String> {
        Ok(self.selection.lock().expect("selection").clone())
    }

    fn select(
        &self,
        panel: Option<PanelSide>,
        mask: &str,
        mode: SelectMode,
    ) -> Result<usize, String> {
        let side = panel.unwrap_or(*self.active.lock().expect("active"));
        let masks = MaskSet::parse(mask).map_err(|error| error.to_string())?;
        let entries = self.list_directory(Some(side), None)?;
        let mut selected = self.selection.lock().expect("selection");
        match mode {
            SelectMode::Set => {
                selected.clear();
                for entry in &entries {
                    if masks.is_match(&entry.name) {
                        selected.push(entry.path.to_string_lossy().to_string());
                    }
                }
            }
            SelectMode::Add => {
                for entry in &entries {
                    let path = entry.path.to_string_lossy().to_string();
                    if masks.is_match(&entry.name) && !selected.contains(&path) {
                        selected.push(path);
                    }
                }
            }
            SelectMode::Remove => {
                selected.retain(|path| {
                    let name = Path::new(path)
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_default();
                    !masks.is_match(&name)
                });
            }
            SelectMode::InvertAll => {
                let mut inverted = Vec::new();
                for entry in &entries {
                    let path = entry.path.to_string_lossy().to_string();
                    if !selected.contains(&path) {
                        inverted.push(path);
                    }
                }
                *selected = inverted;
            }
        }
        let count = selected.len();
        self.record(format!("select {side} {mask} {mode} -> {count}"));
        Ok(count)
    }

    fn copy(
        &self,
        sources: &[String],
        dest: &str,
        move_it: bool,
        overwrite: bool,
    ) -> Result<HostOpResult, String> {
        let destination = PathBuf::from(dest);
        if !destination.is_dir() {
            return Err(format!("`{dest}` is not a directory"));
        }
        let mut result = HostOpResult::default();
        for source in sources {
            let from = PathBuf::from(source);
            let Some(name) = from.file_name() else {
                return Err(format!("`{source}` has no file name"));
            };
            let to = destination.join(name);
            if to.exists() && !overwrite {
                result.skipped += 1;
                continue;
            }
            if move_it {
                std::fs::rename(&from, &to)
                    .map_err(|error| format!("cannot move `{source}` to `{dest}`: {error}"))?;
            } else {
                copy_tree(&from, &to, overwrite, &mut result)?;
            }
            if from.is_dir() {
                result.dirs += 1;
            } else {
                result.files += 1;
                result.bytes += std::fs::metadata(&to).map(|meta| meta.len()).unwrap_or(0);
            }
        }
        self.record(format!(
            "{} {} -> {dest} ({} files, {} skipped)",
            if move_it { "move" } else { "copy" },
            sources.join(","),
            result.files,
            result.skipped
        ));
        Ok(result)
    }

    fn delete(&self, paths: &[String], use_trash: bool) -> Result<HostOpResult, String> {
        let mut result = HostOpResult::default();
        for path in paths {
            let target = PathBuf::from(path);
            let metadata = std::fs::symlink_metadata(&target)
                .map_err(|error| format!("cannot stat `{path}`: {error}"))?;
            if metadata.is_dir() {
                std::fs::remove_dir_all(&target)
                    .map_err(|error| format!("cannot delete `{path}`: {error}"))?;
                result.dirs += 1;
            } else {
                std::fs::remove_file(&target)
                    .map_err(|error| format!("cannot delete `{path}`: {error}"))?;
                result.files += 1;
                result.bytes += metadata.len();
            }
        }
        self.record(format!("delete {} (trash={use_trash})", paths.join(",")));
        Ok(result)
    }

    fn create_directory(&self, path: &str) -> Result<(), String> {
        std::fs::create_dir_all(path)
            .map_err(|error| format!("cannot create `{path}`: {error}"))?;
        self.record(format!("mkdir {path}"));
        Ok(())
    }

    fn rename(&self, from: &str, to: &str) -> Result<(), String> {
        std::fs::rename(from, to).map_err(|error| format!("cannot rename `{from}`: {error}"))?;
        self.record(format!("rename {from} -> {to}"));
        Ok(())
    }

    fn read_text(&self, path: &str, max_bytes: usize) -> Result<String, String> {
        let bytes =
            std::fs::read(path).map_err(|error| format!("cannot read `{path}`: {error}"))?;
        let mut limit = max_bytes.min(bytes.len());
        while limit > 0 && !is_char_boundary(&bytes, limit) {
            limit -= 1;
        }
        Ok(String::from_utf8_lossy(&bytes[..limit]).to_string())
    }

    fn write_text(&self, path: &str, content: &str) -> Result<u64, String> {
        std::fs::write(path, content).map_err(|error| format!("cannot write `{path}`: {error}"))?;
        self.record(format!("write {path} ({} bytes)", content.len()));
        Ok(content.len() as u64)
    }

    fn disk_space(&self, path: &str) -> Result<Option<rc_core::drive::Space>, String> {
        if !Path::new(path).exists() {
            return Err(format!("`{path}` does not exist"));
        }
        Ok(Some(rc_core::drive::Space {
            total: 1_000_000_000,
            available: 250_000_000,
        }))
    }

    fn search(&self, request: &SearchRequest) -> Result<Vec<String>, String> {
        let masks = MaskSet::parse_or_all(&request.name_mask).map_err(|error| error.to_string())?;
        let mut found = Vec::new();
        let mut pending = vec![PathBuf::from(&request.root)];
        while let Some(directory) = pending.pop() {
            let read = std::fs::read_dir(&directory)
                .map_err(|error| format!("cannot read `{}`: {error}", directory.display()))?;
            for item in read {
                if found.len() >= request.max_results {
                    return Ok(found);
                }
                let item = item.map_err(|error| format!("cannot read an entry: {error}"))?;
                let path = item.path();
                let name = item.file_name().to_string_lossy().to_string();
                if path.is_dir() {
                    if request.recursive {
                        pending.push(path);
                    }
                    continue;
                }
                let name_matches = if request.case_sensitive {
                    request
                        .name_mask
                        .split([';', ',', '|'])
                        .filter(|part| !part.trim().is_empty())
                        .any(|part| {
                            Pattern::with_case_sensitivity(part.trim(), true)
                                .map(|pattern| pattern.is_match(&name))
                                .unwrap_or(false)
                        })
                } else {
                    masks.is_match(&name)
                };
                if !name_matches {
                    continue;
                }
                if let Some(needle) = &request.content {
                    let text = String::from_utf8_lossy(&std::fs::read(&path).unwrap_or_default())
                        .to_string();
                    let hit = if request.case_sensitive {
                        text.contains(needle)
                    } else {
                        text.to_lowercase().contains(&needle.to_lowercase())
                    };
                    if !hit {
                        continue;
                    }
                }
                found.push(path.to_string_lossy().to_string());
            }
        }
        found.sort();
        self.record(format!("search {} {}", request.root, request.name_mask));
        Ok(found)
    }

    fn help_topics(&self) -> Vec<(String, String)> {
        vec![
            (
                "navigation".to_string(),
                "Navigating the panels".to_string(),
            ),
            (
                "copying".to_string(),
                "Copying and moving files".to_string(),
            ),
            ("search".to_string(), "Finding files".to_string()),
        ]
    }

    fn help_body(&self, topic: &str, language: &str) -> Option<String> {
        let english = match topic {
            "navigation" => {
                "# Navigating\n\nUse Tab to switch panels and Ctrl+PgDn to enter a directory."
            }
            "copying" => {
                "# Copying\n\nF5 copies to the other panel, F6 moves, F7 creates a directory."
            }
            "search" => "# Searching\n\nAlt+F7 opens the find dialog.",
            _ => return None,
        };
        match language {
            "en" => Some(english.to_string()),
            "cs" => Some(format!("# {topic}\n\nČeská nápověda pro téma `{topic}`.")),
            _ => None,
        }
    }

    fn current_language(&self) -> String {
        self.language.lock().expect("language").clone()
    }

    fn available_languages(&self) -> Vec<String> {
        vec!["en".to_string(), "cs".to_string()]
    }
}

impl LanguageHost for FakeHost {
    fn set_language(&self, language: &str) -> Result<(), String> {
        if !self
            .available_languages()
            .iter()
            .any(|known| known == language)
        {
            return Err(format!("`{language}` is not installed"));
        }
        *self.language.lock().expect("language") = language.to_string();
        self.record(format!("set_language {language}"));
        Ok(())
    }
}

fn is_char_boundary(bytes: &[u8], index: usize) -> bool {
    index == 0 || index >= bytes.len() || (bytes[index] & 0b1100_0000) != 0b1000_0000
}

/// Recursively copy a file or directory tree.
fn copy_tree(
    from: &Path,
    to: &Path,
    overwrite: bool,
    result: &mut HostOpResult,
) -> Result<(), String> {
    if from.is_dir() {
        std::fs::create_dir_all(to)
            .map_err(|error| format!("cannot create `{}`: {error}", to.display()))?;
        for item in std::fs::read_dir(from)
            .map_err(|error| format!("cannot read `{}`: {error}", from.display()))?
        {
            let item = item.map_err(|error| format!("cannot read an entry: {error}"))?;
            let name = item.file_name();
            let target = to.join(name);
            if target.exists() && !overwrite {
                result.skipped += 1;
                continue;
            }
            copy_tree(&item.path(), &target, overwrite, result)?;
        }
    } else {
        std::fs::copy(from, to)
            .map_err(|error| format!("cannot copy `{}`: {error}", from.display()))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Confirmers
// ---------------------------------------------------------------------------

/// Approves or denies every destructive call and remembers what it was asked.
#[derive(Debug, Default)]
pub struct TestConfirmer {
    pub allow: bool,
    pub calls: Mutex<Vec<String>>,
}

impl TestConfirmer {
    /// A confirmer that approves everything.
    pub fn allowing() -> Self {
        TestConfirmer {
            allow: true,
            calls: Mutex::new(Vec::new()),
        }
    }

    /// A confirmer that denies everything (the user pressed "No").
    pub fn denying() -> Self {
        TestConfirmer {
            allow: false,
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Tool names this confirmer was asked about.
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("calls").clone()
    }
}

/// A confirmer whose UI callback blows up; the server must survive it.
#[derive(Debug, Default)]
pub struct PanickingConfirmer;

impl Confirmer for PanickingConfirmer {
    fn confirm(&self, _tool: &str, _args: &Value) -> bool {
        panic!("the approval dialog is broken");
    }
}

impl Confirmer for TestConfirmer {
    fn confirm(&self, tool: &str, _args: &Value) -> bool {
        self.calls.lock().expect("calls").push(tool.to_string());
        self.allow
    }
}

// ---------------------------------------------------------------------------
// Scripted peer for client tests
// ---------------------------------------------------------------------------

/// The reader half of a scripted peer: it blocks until the test (or a fake
/// server thread) pushes bytes, and reports end-of-file once every sender is
/// gone.
pub struct ScriptReader {
    rx: Receiver<Vec<u8>>,
    buffer: Vec<u8>,
    position: usize,
}

impl ScriptReader {
    fn fill(&mut self) -> bool {
        while self.position >= self.buffer.len() {
            match self.rx.recv() {
                Ok(chunk) => {
                    self.buffer = chunk;
                    self.position = 0;
                    if self.buffer.is_empty() {
                        return false;
                    }
                }
                Err(_) => return false,
            }
        }
        true
    }
}

impl Read for ScriptReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if !self.fill() {
            return Ok(0);
        }
        let take = out.len().min(self.buffer.len() - self.position);
        out[..take].copy_from_slice(&self.buffer[self.position..self.position + take]);
        self.position += take;
        Ok(take)
    }
}

impl BufRead for ScriptReader {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if !self.fill() {
            return Ok(&[]);
        }
        Ok(&self.buffer[self.position..])
    }

    fn consume(&mut self, amount: usize) {
        self.position = (self.position + amount).min(self.buffer.len());
    }
}

/// The writer half: records everything the client sent.
#[derive(Clone, Default)]
pub struct RecordingWriter {
    pub bytes: Arc<Mutex<Vec<u8>>>,
}

impl RecordingWriter {
    /// Everything the client wrote, as text.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes.lock().expect("bytes")).to_string()
    }

    /// Everything the client wrote, parsed as JSON objects, in order.
    pub fn messages(&self) -> Vec<Value> {
        self.text()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).expect("the client only writes JSON"))
            .collect()
    }
}

impl Write for RecordingWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.bytes.lock().expect("bytes").extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A scripted MCP server: it feeds JSON-RPC responses to the client on demand.
pub struct ScriptPeer {
    to_client: Sender<Vec<u8>>,
}

impl ScriptPeer {
    /// Create the two halves: pass the reader and writer to
    /// [`rc_agent::McpClient::connect_with_io`] and keep the peer in the test.
    pub fn new() -> (ScriptReader, RecordingWriter, ScriptPeer) {
        let (to_client, rx) = mpsc::channel();
        (
            ScriptReader {
                rx,
                buffer: Vec::new(),
                position: 0,
            },
            RecordingWriter::default(),
            ScriptPeer { to_client },
        )
    }

    /// Send one JSON value as one newline-terminated line.
    pub fn send(&self, value: &Value) {
        let mut line = value.to_string();
        line.push('\n');
        self.to_client
            .send(line.into_bytes())
            .expect("the client is still reading");
    }

    /// Send a successful JSON-RPC response.
    pub fn respond(&self, id: u64, result: Value) {
        self.send(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        }));
    }

    /// Send a JSON-RPC error response.
    pub fn fail(&self, id: u64, code: i64, message: &str) {
        self.send(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message },
        }));
    }

    /// Answer the client's `initialize` request (id 1).
    pub fn respond_initialize(&self) {
        self.respond(
            1,
            serde_json::json!({
                "protocolVersion": rc_agent::PROTOCOL_VERSION,
                "capabilities": { "tools": {}, "resources": {}, "logging": {} },
                "serverInfo": { "name": "scripted", "version": "9.9.9" },
            }),
        );
    }

    /// Answer the client's `tools/list` request (id 2).
    pub fn respond_tools(&self, tools: Value) {
        self.respond(2, serde_json::json!({ "tools": tools }));
    }

    /// Complete the handshake with a single tool.
    pub fn respond_handshake(&self) {
        self.respond_initialize();
        self.respond_tools(serde_json::json!([{
            "name": "echo",
            "title": "Echo",
            "description": "Echo text back.",
            "inputSchema": { "type": "object", "properties": { "text": { "type": "string" } } },
        }]));
    }
}
