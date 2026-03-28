use std::path::Path;

use crate::error::{Result, SearchError};
use crate::indexer::schema::StructuralMeta;
use crate::parser::ParsedDoc;

/// Parse a PDF file using pdf-extract, with lopdf as a fallback.
pub fn parse_pdf(path: &Path) -> Result<ParsedDoc> {
    let text = extract_pdf_text(path).map_err(|e| SearchError::Parse {
        path: path.to_path_buf(),
        reason: e,
    })?;

    let title = extract_pdf_title(path);
    let snippet = text.chars().take(256).collect();

    Ok(ParsedDoc {
        text,
        title,
        snippet,
        metadata: StructuralMeta::default(),
    })
}

/// Try pdf-extract first; fall back to lopdf manual page iteration.
fn extract_pdf_text(path: &Path) -> std::result::Result<String, String> {
    // Primary: pdf-extract (higher quality text extraction)
    match pdf_extract::extract_text(path) {
        Ok(text) if !text.trim().is_empty() => return Ok(normalize_pdf_text(text)),
        Ok(_) => {} // empty result, try fallback
        Err(_) => {} // error, try fallback
    }

    // Fallback: lopdf manual extraction
    extract_via_lopdf(path)
}

fn extract_via_lopdf(path: &Path) -> std::result::Result<String, String> {
    let doc = lopdf::Document::load(path).map_err(|e| format!("lopdf: {}", e))?;

    let mut text = String::new();
    let pages = doc.get_pages();

    for (page_num, _) in pages {
        match doc.extract_text(&[page_num]) {
            Ok(page_text) => {
                text.push_str(&page_text);
                text.push('\n');
            }
            Err(_) => continue,
        }
    }

    if text.trim().is_empty() {
        Err("PDF appears to be image-based or contains no extractable text".to_string())
    } else {
        Ok(normalize_pdf_text(text))
    }
}

/// Normalize PDF text: collapse runs of whitespace, fix line breaks.
fn normalize_pdf_text(text: String) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_was_space = false;

    for c in text.chars() {
        if c == '\r' {
            continue;
        }
        if c == '\n' {
            if !prev_was_space {
                out.push(' ');
                prev_was_space = true;
            }
        } else if c.is_whitespace() {
            if !prev_was_space {
                out.push(' ');
                prev_was_space = true;
            }
        } else {
            out.push(c);
            prev_was_space = false;
        }
    }

    out.trim().to_string()
}

fn extract_pdf_title(path: &Path) -> Option<String> {
    // Try lopdf info dict for a Title field
    if let Ok(doc) = lopdf::Document::load(path) {
        if let Ok(info) = doc.get_object(doc.trailer.get(b"Info").ok()?.as_reference().ok()?) {
            if let Ok(dict) = info.as_dict() {
                if let Ok(title_obj) = dict.get(b"Title") {
                    if let Ok(title) = title_obj.as_str() {
                        let s = String::from_utf8_lossy(title).trim().to_string();
                        if !s.is_empty() {
                            return Some(s);
                        }
                    }
                }
            }
        }
    }

    // Fall back to filename stem
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
}
