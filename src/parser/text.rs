use std::path::Path;

use crate::error::{Result, SearchError};
use crate::parser::ParsedDoc;
use crate::parser::walker::FileKind;
use crate::indexer::schema::StructuralMeta;

/// Parse a plain text or Markdown file.
pub fn parse_text(path: &Path, kind: &FileKind) -> Result<ParsedDoc> {
    let raw = read_with_fallback(path)?;

    let (text, title) = match kind {
        FileKind::Markdown => extract_markdown(raw),
        _ => (raw, None),
    };

    let snippet = text.chars().take(256).collect();
    let metadata = StructuralMeta::default();

    Ok(ParsedDoc {
        text,
        title,
        snippet,
        metadata,
    })
}

/// Read file as UTF-8, falling back to Latin-1 on failure.
fn read_with_fallback(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).map_err(|e| SearchError::Parse {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;

    // Strip UTF-8 BOM if present
    let bytes = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else {
        &bytes
    };

    match std::str::from_utf8(bytes) {
        Ok(s) => Ok(s.to_string()),
        Err(_) => {
            // Latin-1 fallback: every byte maps directly to a Unicode code point
            Ok(bytes.iter().map(|&b| b as char).collect())
        }
    }
}

/// Extract text and title from Markdown, stripping YAML frontmatter.
fn extract_markdown(raw: String) -> (String, Option<String>) {
    let mut text = raw.as_str();
    let mut title = None;

    // Strip YAML frontmatter (--- ... ---)
    if text.starts_with("---") {
        if let Some(end) = text[3..].find("\n---") {
            text = &text[3 + end + 4..];
        }
    }

    // Extract first heading as title
    for line in text.lines() {
        let line = line.trim();
        if let Some(heading) = line.strip_prefix("# ") {
            title = Some(heading.trim().to_string());
            break;
        }
    }

    // Remove Markdown syntax for cleaner text
    let cleaned = strip_markdown_syntax(text);

    (cleaned, title)
}

/// Minimal Markdown syntax stripping: removes `#`, `*`, `_`, backticks, links.
fn strip_markdown_syntax(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for line in s.lines() {
        let line = line.trim_start_matches('#').trim();
        // Strip inline markers naively (good enough for indexing)
        let line = line.replace("**", "").replace("__", "").replace('`', "");
        // Strip markdown links: [text](url) → text
        let line = strip_md_links(&line);
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn strip_md_links(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('[') {
        out.push_str(&rest[..start]);
        rest = &rest[start + 1..];
        if let Some(end_bracket) = rest.find("](") {
            let link_text = &rest[..end_bracket];
            rest = &rest[end_bracket + 2..];
            if let Some(end_paren) = rest.find(')') {
                out.push_str(link_text);
                rest = &rest[end_paren + 1..];
            } else {
                out.push('[');
                out.push_str(link_text);
            }
        } else {
            out.push('[');
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frontmatter_stripped() {
        let md = "---\ntitle: Test\n---\n# Hello\nWorld".to_string();
        let (text, title) = extract_markdown(md);
        assert_eq!(title, Some("Hello".to_string()));
        assert!(text.contains("World"));
        assert!(!text.contains("title: Test"));
    }

    #[test]
    fn test_md_links_stripped() {
        let result = strip_md_links("See [docs](https://example.com) for details");
        assert_eq!(result, "See docs for details");
    }
}
