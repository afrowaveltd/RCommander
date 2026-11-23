//! An in-process FTP server for the test suite.
//!
//! It speaks just enough of RFC 959 + RFC 3659 to exercise every code path of
//! [`super::FtpClient`]: `USER`/`PASS`, `SYST`, `FEAT`, `TYPE`, `PWD`, `CWD`,
//! `PASV`/`EPSV` (a real data listener) and `PORT`/`EPRT` (the server connects
//! back), `LIST`, `MLSD`, `MLST`, `SIZE`, `MDTM`, `RETR`, `STOR`, `DELE`, `MKD`,
//! `RMD`, `RNFR`/`RNTO`, `ABOR`, `NOOP` and `QUIT`, plus `500` for anything
//! else.
//!
//! Everything is bound to `127.0.0.1:0`, so tests never touch the network, and
//! [`MockServer`] joins every thread on drop.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rc_core::entry::unix_mode_string;

use crate::time::civil_from_timestamp;

/// Fixed modification time used by everything the mock creates
/// (2010-01-01T00:00:00Z), so assertions are stable.
pub(crate) const MTIME: i64 = 1_262_304_000;

/// One file of the virtual file system.
#[derive(Clone, Debug)]
pub(crate) struct MockFile {
    /// Contents.
    pub(crate) data: Vec<u8>,
    /// Modification time in unix seconds.
    pub(crate) mtime: i64,
    /// Unix permission bits.
    pub(crate) mode: u32,
}

impl MockFile {
    /// A file with the default timestamp and mode `0644`.
    fn new(data: &[u8]) -> Self {
        MockFile {
            data: data.to_vec(),
            mtime: MTIME,
            mode: 0o644,
        }
    }
}

/// Something the tests can ask the server to get wrong.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Behaviour {
    /// Answer `PASS` with `530 Login incorrect`.
    pub(crate) auth_fail: bool,
    /// Answer `MLSD` with `500 Unknown command`.
    pub(crate) no_mlsd: bool,
    /// Answer `MLSD` with lines that carry no usable facts.
    pub(crate) junk_mlsd: bool,
    /// Answer `MLST` with `500 Unknown command`.
    pub(crate) no_mlst: bool,
    /// Answer `MDTM` with `500 Unknown command`.
    pub(crate) no_mdtm: bool,
    /// Trickle `RETR` data in small chunks so a test can cancel mid-transfer.
    pub(crate) throttle: bool,
}

/// The virtual file system of the mock server, using absolute paths.
#[derive(Clone, Debug)]
pub(crate) struct MockFs {
    /// Regular files.
    pub(crate) files: BTreeMap<String, MockFile>,
    /// Directories (always contains `/`).
    pub(crate) dirs: BTreeSet<String>,
    /// Symlinks and their targets.
    pub(crate) links: BTreeMap<String, String>,
}

impl Default for MockFs {
    fn default() -> Self {
        let mut dirs = BTreeSet::new();
        dirs.insert("/".to_string());
        MockFs {
            files: BTreeMap::new(),
            dirs,
            links: BTreeMap::new(),
        }
    }
}

impl MockFs {
    /// An empty file system with just `/`.
    pub(crate) fn new() -> Self {
        MockFs::default()
    }

    /// Add a directory (and its parents).
    pub(crate) fn dir(mut self, path: &str) -> Self {
        self.make_dir(path);
        self
    }

    /// Add a file with default metadata.
    pub(crate) fn file(mut self, path: &str, data: &[u8]) -> Self {
        self.write_file(path, data);
        self
    }

    /// Add a file with an explicit modification time.
    pub(crate) fn file_at(mut self, path: &str, data: &[u8], mtime: i64) -> Self {
        self.write_file(path, data);
        if let Some(file) = self.files.get_mut(path) {
            file.mtime = mtime;
        }
        self
    }

    /// Add a symlink.
    pub(crate) fn link(mut self, path: &str, target: &str) -> Self {
        self.make_dir(&parent_of(path));
        self.links.insert(path.to_string(), target.to_string());
        self
    }

    /// Create a directory and every missing parent.
    fn make_dir(&mut self, path: &str) {
        let path = normalize(path);
        let mut current = String::new();
        for part in path.split('/').filter(|part| !part.is_empty()) {
            current.push('/');
            current.push_str(part);
            self.dirs.insert(current.clone());
        }
    }

    /// Write a file, creating the parent directory.
    fn write_file(&mut self, path: &str, data: &[u8]) {
        let path = normalize(path);
        self.make_dir(&parent_of(&path));
        self.files.insert(path, MockFile::new(data));
    }

    /// Resolve `path` to a regular file, following symlinks like a real server.
    fn lookup_file(&self, path: &str) -> Option<&MockFile> {
        let mut current = normalize(path);
        for _ in 0..8 {
            if let Some(file) = self.files.get(&current) {
                return Some(file);
            }
            let target = self.links.get(&current)?;
            current = resolve(&parent_of(&current), target);
        }
        None
    }

    /// Immediate children of `dir` as `(name, kind)`.
    fn children(&self, dir: &str) -> Vec<(String, Kind)> {
        let mut out = Vec::new();
        for path in self
            .dirs
            .iter()
            .chain(self.files.keys())
            .chain(self.links.keys())
        {
            if path.as_str() == "/" {
                continue;
            }
            if parent_of(path) == dir {
                let name = path.rsplit('/').next().unwrap_or_default().to_string();
                let kind = if self.dirs.contains(path) {
                    Kind::Dir
                } else if self.links.contains_key(path) {
                    Kind::Link
                } else {
                    Kind::File
                };
                out.push((name, kind));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out.dedup_by(|a, b| a.0 == b.0);
        out
    }
}

/// The kind of a virtual entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Regular file.
    File,
    /// Directory.
    Dir,
    /// Symlink.
    Link,
}

/// Everything the connection handlers share.
struct Shared {
    /// The virtual file system.
    fs: Mutex<MockFs>,
    /// Requested misbehaviour.
    behaviour: Behaviour,
    /// Set by [`MockServer::drop`].
    stop: AtomicBool,
    /// Number of `MLSD` commands received.
    mlsd_calls: AtomicUsize,
    /// Number of `LIST` commands received.
    list_calls: AtomicUsize,
    /// Number of `MLST` commands received.
    mlst_calls: AtomicUsize,
}

/// A running mock FTP server. Dropping it stops the threads.
pub(crate) struct MockServer {
    /// Address the server listens on.
    pub(crate) address: SocketAddr,
    shared: Arc<Shared>,
    accept: Option<JoinHandle<()>>,
    workers: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

impl MockServer {
    /// Start a well-behaved server on top of `fs`.
    pub(crate) fn start(fs: MockFs) -> MockServer {
        MockServer::with_behaviour(fs, Behaviour::default())
    }

    /// Start a server that misbehaves in the requested ways.
    pub(crate) fn with_behaviour(fs: MockFs, behaviour: Behaviour) -> MockServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind the mock FTP server");
        listener
            .set_nonblocking(true)
            .expect("make the listener non-blocking");
        let address = listener.local_addr().expect("read the listener address");
        let shared = Arc::new(Shared {
            fs: Mutex::new(fs),
            behaviour,
            stop: AtomicBool::new(false),
            mlsd_calls: AtomicUsize::new(0),
            list_calls: AtomicUsize::new(0),
            mlst_calls: AtomicUsize::new(0),
        });
        let workers: Arc<Mutex<Vec<JoinHandle<()>>>> = Arc::new(Mutex::new(Vec::new()));

        let accept_shared = Arc::clone(&shared);
        let accept_workers = Arc::clone(&workers);
        let accept = thread::spawn(move || {
            while !accept_shared.stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((socket, _peer)) => {
                        let shared = Arc::clone(&accept_shared);
                        let handle = thread::spawn(move || {
                            let _ = serve(socket, &shared);
                        });
                        if let Ok(mut workers) = accept_workers.lock() {
                            workers.push(handle);
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
        });

        MockServer {
            address,
            shared,
            accept: Some(accept),
            workers,
        }
    }

    /// TCP port the server listens on.
    pub(crate) fn port(&self) -> u16 {
        self.address.port()
    }

    /// How often `MLSD` was sent to the server.
    pub(crate) fn mlsd_calls(&self) -> usize {
        self.shared.mlsd_calls.load(Ordering::SeqCst)
    }

    /// How often `LIST` was sent to the server.
    pub(crate) fn list_calls(&self) -> usize {
        self.shared.list_calls.load(Ordering::SeqCst)
    }

    /// How often `MLST` was sent to the server.
    pub(crate) fn mlst_calls(&self) -> usize {
        self.shared.mlst_calls.load(Ordering::SeqCst)
    }

    /// Contents of a stored file, or `None` when it does not exist.
    pub(crate) fn file(&self, path: &str) -> Option<Vec<u8>> {
        self.shared
            .fs
            .lock()
            .unwrap()
            .files
            .get(path)
            .map(|file| file.data.clone())
    }

    /// Whether a directory exists on the server.
    pub(crate) fn has_dir(&self, path: &str) -> bool {
        self.shared.fs.lock().unwrap().dirs.contains(path)
    }

    /// Whether a file exists on the server.
    pub(crate) fn has_file(&self, path: &str) -> bool {
        self.shared.fs.lock().unwrap().files.contains_key(path)
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        // Wake the accept loop, which is parked in `accept`.
        let _ = TcpStream::connect(self.address);
        if let Some(handle) = self.accept.take() {
            let _ = handle.join();
        }
        let handles: Vec<JoinHandle<()>> = match self.workers.lock() {
            Ok(mut workers) => workers.drain(..).collect(),
            Err(_) => Vec::new(),
        };
        for handle in handles {
            let _ = handle.join();
        }
    }
}

/// Path arithmetic shared by the handlers.
fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    format!("/{}", parts.join("/"))
}

/// Directory part of a normalised absolute path.
fn parent_of(path: &str) -> String {
    let normalised = normalize(path);
    match normalised.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(index) => normalised[..index].to_string(),
    }
}

/// Resolve `argument` against the working directory.
fn resolve(cwd: &str, argument: &str) -> String {
    let argument = argument.trim();
    if argument.is_empty() {
        return normalize(cwd);
    }
    if argument.starts_with('/') {
        normalize(argument)
    } else {
        normalize(&format!("{cwd}/{argument}"))
    }
}

/// Month abbreviation for a `LIST` line.
fn month_abbreviation(month: u32) -> &'static str {
    match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "Jan",
    }
}

/// `YYYYMMDDHHMMSS` for the `MDTM`/`MLSD` formats.
fn stamp(timestamp: i64) -> String {
    let (year, month, day, hour, minute, second) = civil_from_timestamp(timestamp);
    format!("{year:04}{month:02}{day:02}{hour:02}{minute:02}{second:02}")
}

/// A `LIST` line for one entry.
fn list_line(name: &str, kind: Kind, file: Option<&MockFile>, target: Option<&str>) -> String {
    let (type_char, mode, size) = match kind {
        Kind::Dir => ('d', 0o755, 4096),
        Kind::Link => ('l', 0o777, target.map(|t| t.len()).unwrap_or(0)),
        Kind::File => (
            '-',
            file.map(|f| f.mode).unwrap_or(0o644),
            file.map(|f| f.data.len()).unwrap_or(0),
        ),
    };
    let mtime = file.map(|f| f.mtime).unwrap_or(MTIME);
    let (year, month, day, ..) = civil_from_timestamp(mtime);
    let date = format!("{} {:>2}  {year}", month_abbreviation(month), day);
    let rendered = match kind {
        Kind::Link => format!("{name} -> {}", target.unwrap_or("")),
        _ => name.to_string(),
    };
    format!(
        "{type_char}{} 1 ftp ftp {size:>8} {date} {rendered}",
        unix_mode_string(mode)
    )
}

/// An `MLSD`/`MLST` fact line for one entry.
fn mlsd_line(name: &str, kind: Kind, file: Option<&MockFile>, target: Option<&str>) -> String {
    let mtime = stamp(file.map(|f| f.mtime).unwrap_or(MTIME));
    match kind {
        Kind::Dir => format!("type=dir;modify={mtime};perm=elcmp;unix.mode=0755; {name}"),
        Kind::Link => format!(
            "type=OS.unix=slink;size={};modify={mtime};unix.linktarget={}; {name}",
            target.map(|t| t.len()).unwrap_or(0),
            target.unwrap_or("")
        ),
        Kind::File => {
            format!(
                "type=file;size={};modify={mtime};perm=adfr;unix.mode={:04o}; {name}",
                file.map(|f| f.data.len()).unwrap_or(0),
                file.map(|f| f.mode).unwrap_or(0o644)
            )
        }
    }
}

/// One client connection.
struct Conn {
    socket: TcpStream,
    cwd: String,
    rename_from: Option<String>,
    data: Option<TcpListener>,
    active_target: Option<SocketAddr>,
    logged_in: bool,
}

impl Conn {
    /// Send a single-line reply.
    fn reply(&mut self, code: u16, text: &str) -> std::io::Result<()> {
        self.socket
            .write_all(format!("{code} {text}\r\n").as_bytes())
    }

    /// Send a multi-line reply (`code-first`, body lines, `code End`).
    fn reply_multiline(&mut self, code: u16, first: &str, lines: &[&str]) -> std::io::Result<()> {
        let mut out = format!("{code}-{first}\r\n");
        for line in lines {
            out.push_str(line);
            out.push_str("\r\n");
        }
        out.push_str(&format!("{code} End\r\n"));
        self.socket.write_all(out.as_bytes())
    }

    /// Read one command line, or `None` when the client went away.
    fn read_command(&mut self, shared: &Shared) -> std::io::Result<Option<String>> {
        let mut line = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            match self.socket.read(&mut byte) {
                Ok(0) => return Ok(None),
                Ok(_) => {
                    if byte[0] == b'\n' {
                        break;
                    }
                    if byte[0] != b'\r' {
                        line.push(byte[0]);
                    }
                    if line.len() > 8 * 1024 {
                        return Ok(None);
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    if shared.stop.load(Ordering::SeqCst) {
                        return Ok(None);
                    }
                }
                Err(_) => return Ok(None),
            }
        }
        Ok(Some(String::from_utf8_lossy(&line).trim().to_string()))
    }

    /// Accept the announced data connection (passive) or connect back (active).
    fn data_socket(&mut self) -> std::io::Result<TcpStream> {
        if let Some(listener) = self.data.take() {
            listener.set_nonblocking(true)?;
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                match listener.accept() {
                    Ok((socket, _peer)) => {
                        let _ = socket.set_nonblocking(false);
                        let _ = socket.set_nodelay(true);
                        let _ = socket.set_write_timeout(Some(Duration::from_secs(5)));
                        return Ok(socket);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if Instant::now() > deadline {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::TimedOut,
                                "no data connection",
                            ));
                        }
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        if let Some(target) = self.active_target.take() {
            return TcpStream::connect_timeout(&target, Duration::from_secs(2));
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::NotConnected,
            "no data connection announced",
        ))
    }

    /// Send a listing over the data connection.
    fn send_lines(&mut self, lines: &[String]) -> std::io::Result<()> {
        let mut socket = match self.data_socket() {
            Ok(socket) => socket,
            Err(_) => return self.reply(425, "Cannot open data connection."),
        };
        for line in lines {
            if socket.write_all(line.as_bytes()).is_err() {
                return self.reply(426, "Transfer aborted.");
            }
            if socket.write_all(b"\r\n").is_err() {
                return self.reply(426, "Transfer aborted.");
            }
        }
        drop(socket);
        self.reply(226, "Transfer complete.")
    }

    /// Serve one command; returns `false` when the connection should close.
    fn handle(&mut self, line: &str, shared: &Shared) -> std::io::Result<bool> {
        let (verb, argument) = match line.split_once(' ') {
            Some((verb, argument)) => (verb.to_ascii_uppercase(), argument.to_string()),
            None => (line.to_ascii_uppercase(), String::new()),
        };
        let behaviour = shared.behaviour;
        match verb.as_str() {
            "USER" => self.reply(331, "Please specify the password.")?,
            "PASS" => {
                if behaviour.auth_fail {
                    self.reply(530, "Login incorrect.")?;
                } else {
                    self.logged_in = true;
                    self.reply(230, "Login successful.")?;
                }
            }
            "SYST" => self.reply(215, "UNIX Type: L8")?,
            "FEAT" => self.reply_multiline(
                211,
                "Features:",
                &[" MLST type*;size*;modify*;", " MLSD", " UTF8"],
            )?,
            "TYPE" => self.reply(200, "Switching to Binary mode.")?,
            "PWD" | "XPWD" => {
                let cwd = self.cwd.clone();
                self.reply(257, &format!("\"{cwd}\" is the current directory"))?;
            }
            "CWD" | "XCWD" => {
                let path = resolve(&self.cwd, &argument);
                let exists = shared
                    .fs
                    .lock()
                    .map(|fs| fs.dirs.contains(&path))
                    .unwrap_or(false);
                if exists {
                    self.cwd = path;
                    self.reply(250, "Directory successfully changed.")?;
                } else {
                    self.reply(550, "Failed to change directory.")?;
                }
            }
            "CDUP" => {
                let parent = parent_of(&self.cwd);
                let exists = shared
                    .fs
                    .lock()
                    .map(|fs| fs.dirs.contains(&parent))
                    .unwrap_or(false);
                if exists {
                    self.cwd = parent;
                    self.reply(250, "Directory successfully changed.")?;
                } else {
                    self.reply(550, "Failed to change directory.")?;
                }
            }
            "PASV" => {
                let listener = TcpListener::bind("127.0.0.1:0")?;
                let port = listener.local_addr()?.port();
                self.data = Some(listener);
                self.reply(
                    227,
                    &format!(
                        "Entering Passive Mode (127,0,0,1,{},{}).",
                        port >> 8,
                        port & 0xFF
                    ),
                )?;
            }
            "EPSV" => {
                let listener = TcpListener::bind("127.0.0.1:0")?;
                let port = listener.local_addr()?.port();
                self.data = Some(listener);
                self.reply(229, &format!("Entering Extended Passive Mode (|||{port}|)"))?;
            }
            "PORT" => match parse_port(&argument) {
                Some(target) => {
                    self.active_target = Some(target);
                    self.reply(200, "PORT command successful.")?;
                }
                None => self.reply(500, "Illegal PORT command.")?,
            },
            "EPRT" => match parse_eprt(&argument) {
                Some(target) => {
                    self.active_target = Some(target);
                    self.reply(200, "EPRT command successful.")?;
                }
                None => self.reply(500, "Illegal EPRT command.")?,
            },
            "NLST" | "LIST" => {
                shared.list_calls.fetch_add(1, Ordering::SeqCst);
                let path = resolve(&self.cwd, &argument);
                let lines = {
                    let fs = shared.fs.lock().unwrap();
                    if !fs.dirs.contains(&path) {
                        None
                    } else {
                        Some(
                            fs.children(&path)
                                .into_iter()
                                .map(|(name, kind)| {
                                    let full = if path == "/" {
                                        format!("/{name}")
                                    } else {
                                        format!("{path}/{name}")
                                    };
                                    let file = fs.files.get(&full);
                                    let target = fs.links.get(&full).map(|target| target.as_str());
                                    if verb == "NLST" {
                                        name.clone()
                                    } else {
                                        list_line(&name, kind, file, target)
                                    }
                                })
                                .collect::<Vec<_>>(),
                        )
                    }
                };
                match lines {
                    Some(lines) => {
                        self.reply(150, "Here comes the directory listing.")?;
                        self.send_lines(&lines)?;
                    }
                    None => self.reply(550, "No such file or directory.")?,
                }
            }
            "MLSD" => {
                shared.mlsd_calls.fetch_add(1, Ordering::SeqCst);
                if behaviour.no_mlsd {
                    self.reply(500, "Unknown command.")?;
                } else if behaviour.junk_mlsd {
                    self.reply(150, "Here comes the directory listing.")?;
                    let lines = vec![
                        "-rw-r--r-- 1 ftp ftp 5 Jan 01 2010 a.txt".to_string(),
                        "garbage line without any facts".to_string(),
                    ];
                    self.send_lines(&lines)?;
                } else {
                    let path = resolve(&self.cwd, &argument);
                    let lines = {
                        let fs = shared.fs.lock().unwrap();
                        if !fs.dirs.contains(&path) {
                            None
                        } else {
                            Some(
                                fs.children(&path)
                                    .into_iter()
                                    .map(|(name, kind)| {
                                        let full = if path == "/" {
                                            format!("/{name}")
                                        } else {
                                            format!("{path}/{name}")
                                        };
                                        let file = fs.files.get(&full);
                                        let target =
                                            fs.links.get(&full).map(|target| target.as_str());
                                        mlsd_line(&name, kind, file, target)
                                    })
                                    .collect::<Vec<_>>(),
                            )
                        }
                    };
                    match lines {
                        Some(lines) => {
                            self.reply(150, "Here comes the directory listing.")?;
                            self.send_lines(&lines)?;
                        }
                        None => self.reply(550, "No such file or directory.")?,
                    }
                }
            }
            "MLST" => {
                shared.mlst_calls.fetch_add(1, Ordering::SeqCst);
                if behaviour.no_mlst {
                    self.reply(500, "Unknown command.")?;
                } else {
                    let path = resolve(&self.cwd, &argument);
                    let line = {
                        let fs = shared.fs.lock().unwrap();
                        if fs.dirs.contains(&path) {
                            Some(mlsd_line(&path, Kind::Dir, None, None))
                        } else if let Some(file) = fs.files.get(&path) {
                            Some(mlsd_line(&path, Kind::File, Some(file), None))
                        } else {
                            fs.links.get(&path).map(|target| {
                                mlsd_line(&path, Kind::Link, None, Some(target.as_str()))
                            })
                        }
                    };
                    match line {
                        Some(line) => self.reply_multiline(
                            250,
                            &format!("Listing {path}"),
                            &[format!(" {line}").as_str()],
                        )?,
                        None => self.reply(550, "No such file or directory.")?,
                    }
                }
            }
            "SIZE" => {
                let path = resolve(&self.cwd, &argument);
                let size = shared
                    .fs
                    .lock()
                    .unwrap()
                    .lookup_file(&path)
                    .map(|file| file.data.len());
                match size {
                    Some(size) => self.reply(213, &size.to_string())?,
                    None => self.reply(550, "Could not get file size.")?,
                }
            }
            "MDTM" => {
                if behaviour.no_mdtm {
                    return self.reply(500, "Unknown command.").map(|_| true);
                }
                let path = resolve(&self.cwd, &argument);
                let mtime = shared
                    .fs
                    .lock()
                    .unwrap()
                    .lookup_file(&path)
                    .map(|file| file.mtime);
                match mtime {
                    Some(mtime) => self.reply(213, &stamp(mtime))?,
                    None => self.reply(550, "No such file or directory.")?,
                }
            }
            "RETR" => {
                let path = resolve(&self.cwd, &argument);
                let data = shared
                    .fs
                    .lock()
                    .unwrap()
                    .lookup_file(&path)
                    .map(|file| file.data.clone());
                match data {
                    Some(data) => {
                        self.reply(150, "Opening BINARY mode data connection.")?;
                        let socket = match self.data_socket() {
                            Ok(socket) => socket,
                            Err(_) => {
                                return self
                                    .reply(425, "Cannot open data connection.")
                                    .map(|_| true);
                            }
                        };
                        if behaviour.throttle {
                            let mut socket = socket;
                            let chunk = 2048;
                            let mut failed = false;
                            for piece in data.chunks(chunk) {
                                if socket.write_all(piece).is_err() {
                                    failed = true;
                                    break;
                                }
                                thread::sleep(Duration::from_millis(2));
                            }
                            drop(socket);
                            if failed {
                                self.reply(426, "Transfer aborted.")?;
                            } else {
                                self.reply(226, "Transfer complete.")?;
                            }
                        } else {
                            let mut socket = socket;
                            let _ = socket.write_all(&data);
                            drop(socket);
                            self.reply(226, "Transfer complete.")?;
                        }
                    }
                    None => self.reply(550, "Failed to open file.")?,
                }
            }
            "STOR" => {
                let path = resolve(&self.cwd, &argument);
                self.reply(150, "Ok to send data.")?;
                let mut socket = match self.data_socket() {
                    Ok(socket) => socket,
                    Err(_) => {
                        return self
                            .reply(425, "Cannot open data connection.")
                            .map(|_| true);
                    }
                };
                let mut data = Vec::new();
                let mut buffer = [0u8; 8192];
                loop {
                    match socket.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(read) => data.extend_from_slice(&buffer[..read]),
                        Err(_) => break,
                    }
                }
                drop(socket);
                {
                    let mut fs = shared.fs.lock().unwrap();
                    let parent = parent_of(&path);
                    fs.make_dir(&parent);
                    fs.files.insert(
                        path,
                        MockFile {
                            data,
                            mtime: MTIME,
                            mode: 0o644,
                        },
                    );
                }
                self.reply(226, "Transfer complete.")?;
            }
            "DELE" => {
                let path = resolve(&self.cwd, &argument);
                let removed = shared.fs.lock().unwrap().files.remove(&path).is_some();
                if removed {
                    self.reply(250, "Delete operation successful.")?;
                } else {
                    self.reply(550, "No such file or directory.")?;
                }
            }
            "MKD" | "XMKD" => {
                let path = resolve(&self.cwd, &argument);
                let mut fs = shared.fs.lock().unwrap();
                if fs.dirs.contains(&path) {
                    drop(fs);
                    self.reply(550, "Directory already exists.")?;
                } else {
                    fs.make_dir(&path);
                    drop(fs);
                    self.reply(257, &format!("\"{path}\" created"))?;
                }
            }
            "RMD" | "XRMD" => {
                let path = resolve(&self.cwd, &argument);
                let removed = shared.fs.lock().unwrap().dirs.remove(&path);
                if removed {
                    self.reply(250, "Remove directory operation successful.")?;
                } else {
                    self.reply(550, "No such directory.")?;
                }
            }
            "RNFR" => {
                let path = resolve(&self.cwd, &argument);
                let exists = {
                    let fs = shared.fs.lock().unwrap();
                    fs.files.contains_key(&path) || fs.dirs.contains(&path)
                };
                if exists {
                    self.rename_from = Some(path);
                    self.reply(350, "Ready for RNTO.")?;
                } else {
                    self.reply(550, "No such file or directory.")?;
                }
            }
            "RNTO" => {
                let Some(from) = self.rename_from.take() else {
                    return self.reply(503, "RNFR required first.").map(|_| true);
                };
                let to = resolve(&self.cwd, &argument);
                let renamed = {
                    let mut fs = shared.fs.lock().unwrap();
                    if let Some(file) = fs.files.remove(&from) {
                        fs.make_dir(&parent_of(&to));
                        fs.files.insert(to.clone(), file);
                        true
                    } else if fs.dirs.remove(&from) {
                        fs.make_dir(&to);
                        true
                    } else if let Some(target) = fs.links.remove(&from) {
                        fs.links.insert(to.clone(), target);
                        true
                    } else {
                        false
                    }
                };
                if renamed {
                    self.reply(250, "Rename successful.")?;
                } else {
                    self.reply(550, "Rename failed.")?;
                }
            }
            "NOOP" => self.reply(200, "NOOP successful.")?,
            "ABOR" => self.reply(226, "ABOR command successful.")?,
            "QUIT" => {
                self.reply(221, "Goodbye.")?;
                return Ok(false);
            }
            _ => self.reply(500, "Unknown command.")?,
        }
        Ok(true)
    }
}

/// `PORT h1,h2,h3,h4,p1,p2` → address.
fn parse_port(argument: &str) -> Option<SocketAddr> {
    let numbers: Vec<u8> = argument
        .split(',')
        .map(|part| part.trim().parse().ok())
        .collect::<Option<Vec<u8>>>()?;
    if numbers.len() != 6 {
        return None;
    }
    let ip = std::net::Ipv4Addr::new(numbers[0], numbers[1], numbers[2], numbers[3]);
    let port = (u16::from(numbers[4]) << 8) | u16::from(numbers[5]);
    Some(SocketAddr::new(std::net::IpAddr::V4(ip), port))
}

/// `EPRT |1|127.0.0.1|port|` → address.
fn parse_eprt(argument: &str) -> Option<SocketAddr> {
    let mut parts = argument.split('|');
    parts.next()?; // leading delimiter
    let family: &str = parts.next()?;
    let host = parts.next()?;
    let port: u16 = parts.next()?.parse().ok()?;
    let ip: std::net::IpAddr = host.parse().ok()?;
    match (family, ip) {
        ("1", std::net::IpAddr::V4(_)) | ("2", std::net::IpAddr::V6(_)) => {
            Some(SocketAddr::new(ip, port))
        }
        _ => None,
    }
}

/// Serve one control connection until the client leaves.
fn serve(socket: TcpStream, shared: &Shared) -> std::io::Result<()> {
    let _ = socket.set_nodelay(true);
    let _ = socket.set_read_timeout(Some(Duration::from_millis(20)));
    let _ = socket.set_write_timeout(Some(Duration::from_secs(5)));
    let mut conn = Conn {
        socket,
        cwd: "/".to_string(),
        rename_from: None,
        data: None,
        active_target: None,
        logged_in: false,
    };
    conn.reply(220, "Mock FTP server ready")?;
    loop {
        let Some(line) = conn.read_command(shared)? else {
            return Ok(());
        };
        if line.is_empty() {
            continue;
        }
        if !conn.handle(&line, shared)? {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_normalised() {
        assert_eq!(normalize("/a/b/../c"), "/a/c");
        assert_eq!(normalize("//a//b//"), "/a/b");
        assert_eq!(normalize("/"), "/");
        assert_eq!(normalize("/.."), "/");
        assert_eq!(parent_of("/a/b"), "/a");
        assert_eq!(parent_of("/a"), "/");
        assert_eq!(resolve("/pub", "incoming"), "/pub/incoming");
        assert_eq!(resolve("/pub", "/etc"), "/etc");
        assert_eq!(resolve("/pub", ".."), "/");
        assert_eq!(resolve("/pub", ""), "/pub");
    }

    #[test]
    fn port_and_eprt_arguments_are_parsed() {
        assert_eq!(
            parse_port("127,0,0,1,200,45"),
            Some("127.0.0.1:51245".parse().unwrap())
        );
        assert_eq!(parse_port("127,0,0,1,200"), None);
        assert_eq!(parse_port("a,b,c,d,e,f"), None);
        assert_eq!(
            parse_eprt("|1|127.0.0.1|2121|"),
            Some("127.0.0.1:2121".parse().unwrap())
        );
        assert_eq!(
            parse_eprt("|2|::1|2121|"),
            Some("[::1]:2121".parse().unwrap())
        );
        assert_eq!(parse_eprt("|1|::1|2121|"), None);
    }

    #[test]
    fn listings_are_rendered_in_both_formats() {
        let fs = MockFs::new()
            .dir("/pub")
            .dir("/pub/sub")
            .file("/pub/a.txt", b"hello")
            .link("/pub/current", "/pub/a.txt");
        let children = fs.children("/pub");
        assert_eq!(children.len(), 3);
        assert_eq!(children[0].0, "a.txt");
        let file = fs.files.get("/pub/a.txt").unwrap();
        let list = list_line("a.txt", Kind::File, Some(file), None);
        assert!(list.starts_with("-rw-r--r-- 1 ftp ftp"), "{list}");
        assert!(list.ends_with(" a.txt"), "{list}");
        assert!(list.contains("Jan  1  2010"), "{list}");
        let mlsd = mlsd_line("a.txt", Kind::File, Some(file), None);
        assert_eq!(
            mlsd,
            "type=file;size=5;modify=20100101000000;perm=adfr;unix.mode=0644; a.txt"
        );
        let link = mlsd_line("current", Kind::Link, None, Some("/pub/a.txt"));
        assert!(link.starts_with("type=OS.unix=slink;"), "{link}");
        assert!(link.ends_with(" current"), "{link}");
    }

    #[test]
    fn the_server_serves_a_greeting_and_shuts_down_cleanly() {
        let server = MockServer::start(MockFs::new().file("/hello.txt", b"hi"));
        let mut socket = TcpStream::connect(server.address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut buffer = [0u8; 64];
        let read = socket.read(&mut buffer).unwrap();
        assert!(String::from_utf8_lossy(&buffer[..read]).starts_with("220 "));
        socket.write_all(b"QUIT\r\n").unwrap();
        let read = socket.read(&mut buffer).unwrap();
        assert!(String::from_utf8_lossy(&buffer[..read]).starts_with("221 "));
        drop(socket);
        drop(server); // joins every thread; must not hang
    }
}
