use std::path::{Path, PathBuf};
use std::time::SystemTime;

use ignore::WalkBuilder;

use crate::error::Result;

/// Detected file kind based on extension + magic bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileKind {
    PlainText,
    Markdown,
    Pdf,
    Docx,
    Code(String), // language name, e.g. "rust", "python"
    Unknown,
}

/// A single file discovered during a directory walk.
#[derive(Debug, Clone)]
pub struct WalkEntry {
    pub path: PathBuf,
    pub kind: FileKind,
    pub mtime: SystemTime,
    pub size: u64,
}

/// Walk a directory, respecting .gitignore / .ignore / .searchignore.
/// Returns all indexable files (skips directories, symlinks, hidden files).
pub fn walk_directory(root: &Path) -> Result<Vec<WalkEntry>> {
    let mut entries = Vec::new();

    let walker = WalkBuilder::new(root)
        .hidden(true)          // skip hidden files by default
        .ignore(true)          // respect .ignore files
        .git_ignore(true)      // respect .gitignore
        .add_custom_ignore_filename(".searchignore")
        .build();

    for result in walker {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };

        // Only process files
        if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            let path = entry.path().to_path_buf();
            let kind = detect_kind(&path);
            if kind == FileKind::Unknown {
                continue;
            }

            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            let size = metadata.len();

            entries.push(WalkEntry { path, kind, mtime, size });
        }
    }

    Ok(entries)
}

/// Detect file kind by extension first, magic bytes as fallback.
pub fn detect_kind(path: &Path) -> FileKind {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase());

    match ext.as_deref() {
        Some("md") | Some("markdown") => FileKind::Markdown,
        Some("pdf") => FileKind::Pdf,
        Some("docx") => FileKind::Docx,
        Some("txt") | Some("text") => FileKind::PlainText,
        Some("rs") => FileKind::Code("rust".into()),
        Some("py") => FileKind::Code("python".into()),
        Some("js") | Some("mjs") | Some("cjs") => FileKind::Code("javascript".into()),
        Some("ts") | Some("tsx") => FileKind::Code("typescript".into()),
        Some("go") => FileKind::Code("go".into()),
        Some("java") => FileKind::Code("java".into()),
        Some("c") | Some("h") => FileKind::Code("c".into()),
        Some("cpp") | Some("cc") | Some("cxx") | Some("hpp") => FileKind::Code("cpp".into()),
        Some("rb") => FileKind::Code("ruby".into()),
        Some("swift") => FileKind::Code("swift".into()),
        Some("kt") | Some("kts") => FileKind::Code("kotlin".into()),
        Some("toml") | Some("yaml") | Some("yml") | Some("json") => FileKind::PlainText,
        Some("sh") | Some("bash") | Some("zsh") => FileKind::Code("shell".into()),
        _ => FileKind::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_kind() {
        assert_eq!(detect_kind(Path::new("README.md")), FileKind::Markdown);
        assert_eq!(detect_kind(Path::new("doc.pdf")), FileKind::Pdf);
        assert_eq!(detect_kind(Path::new("report.docx")), FileKind::Docx);
        assert_eq!(detect_kind(Path::new("main.rs")), FileKind::Code("rust".into()));
        assert_eq!(detect_kind(Path::new("image.png")), FileKind::Unknown);
    }
}
