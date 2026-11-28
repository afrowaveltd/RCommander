//! Optional system PDF renderer. The application does not ship or link Poppler.

use crate::ImageDocument;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// A rendered page and the total number of pages in its document.
#[derive(Debug)]
pub struct PdfPage {
    /// Total page count.
    pub pages: u32,
    /// Rendered RGBA image.
    pub image: ImageDocument,
}

/// A localized failure category, independent of operating-system error text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfError {
    /// Required optional programs were not found.
    MissingRenderer,
    /// The file was unavailable, not regular or exceeded the input limit.
    Input,
    /// The PDF is invalid, encrypted or the selected page cannot be rendered.
    Render,
    /// The renderer exceeded its deadline.
    Timeout,
    /// The caller cancelled the request.
    Cancelled,
}

fn program(name: &str) -> Option<PathBuf> {
    let file = if cfg!(windows) { format!("{name}.exe") } else { name.to_owned() };
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .filter(|dir| dir.is_absolute())
            .map(|dir| dir.join(&file))
            .find(|path| path.is_file())
    })
}

fn run(command: &mut Command, deadline: Instant, cancel: &AtomicBool) -> Result<(), PdfError> {
    if cancel.load(Ordering::Relaxed) { return Err(PdfError::Cancelled); }
    let mut child = command.stdin(Stdio::null()).stderr(Stdio::null())
        .spawn().map_err(|_| PdfError::Render)?;
    loop {
        let error = if cancel.load(Ordering::Relaxed) {
            Some(PdfError::Cancelled)
        } else if Instant::now() >= deadline {
            Some(PdfError::Timeout)
        } else {
            match child.try_wait() {
                Ok(Some(status)) => return if status.success() { Ok(()) } else { Err(PdfError::Render) },
                Ok(None) => None,
                Err(_) => Some(PdfError::Render),
            }
        };
        if let Some(error) = error {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn page_count(info: &str) -> Option<u32> {
    info.lines().find_map(|line| line.strip_prefix("Pages:")
        .and_then(|value| value.trim().parse::<u32>().ok())
        .filter(|count| *count > 0))
}

/// Render one local PDF page (1-based), with an optional already known page
/// count. Requires independently installed pdfinfo and pdftoppm on PATH.
/// Call from a worker, not the UI thread. The subprocess is terminated on
/// cancellation or after 15 seconds. The pixel output is capped to a 1600px
/// square; this is not an OS sandbox or a subprocess memory limit.
pub fn render_pdf_page(path: &Path, page: u32, known_pages: Option<u32>, cancel: &AtomicBool) -> Result<PdfPage, PdfError> {
    let path = path.canonicalize().map_err(|_| PdfError::Input)?;
    let meta = path.metadata().map_err(|_| PdfError::Input)?;
    if !meta.is_file() || meta.len() > 256 * 1024 * 1024 || page == 0 {
        return Err(PdfError::Input);
    }
    let renderer = program("pdftoppm").ok_or(PdfError::MissingRenderer)?;
    let dir = tempfile::tempdir().map_err(|_| PdfError::Input)?;
    let deadline = Instant::now() + Duration::from_secs(15);
    let pages = if let Some(count) = known_pages {
        count
    } else {
        let info = program("pdfinfo").ok_or(PdfError::MissingRenderer)?;
        let info_path = dir.path().join("info");
        let output = std::fs::File::create(&info_path).map_err(|_| PdfError::Input)?;
        run(Command::new(info).current_dir(dir.path()).env("LC_ALL", "C")
            .arg(&path).stdout(Stdio::from(output)), deadline, cancel)?;
        let mut bytes = Vec::new();
        std::fs::File::open(info_path).map_err(|_| PdfError::Input)?
            .take(64 * 1024).read_to_end(&mut bytes).map_err(|_| PdfError::Input)?;
        page_count(&String::from_utf8_lossy(&bytes)).ok_or(PdfError::Render)?
    };
    if page > pages { return Err(PdfError::Render); }
    let prefix = dir.path().join("page");
    run(Command::new(renderer).current_dir(dir.path())
        .args(["-png", "-singlefile", "-scale-to", "1600", "-f"])
        .arg(page.to_string()).arg("-l").arg(page.to_string())
        .arg(&path).arg(&prefix).stdout(Stdio::null()), deadline, cancel)?;
    let output = prefix.with_extension("png");
    if output.metadata().map_err(|_| PdfError::Render)?.len() > 16 * 1024 * 1024 {
        return Err(PdfError::Render);
    }
    let image = crate::load_image(&output, 1600 * 1600).map_err(|_| PdfError::Render)?;
    Ok(PdfPage { pages, image })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_page_count_without_interpreting_other_metadata() {
        assert_eq!(page_count("Title: Pages: 99\nPages: 12\n"), Some(12));
        assert_eq!(page_count("Pages: 0\n"), None);
        assert_eq!(page_count("Pages: invalid\n"), None);
    }

    #[test]
    fn missing_file_and_zero_page_are_refused() {
        let cancel = AtomicBool::new(false);
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(render_pdf_page(&dir.path().join("missing.pdf"), 1, None, &cancel), Err(PdfError::Input)));
        let path = dir.path().join("file.pdf");
        std::fs::write(&path, b"%PDF-1.4").unwrap();
        assert!(matches!(render_pdf_page(&path, 0, None, &cancel), Err(PdfError::Input)));
    }
}
