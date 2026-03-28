use std::io::Read;
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::error::{Result, SearchError};
use crate::indexer::schema::StructuralMeta;
use crate::parser::ParsedDoc;

/// Parse a .docx file by reading word/document.xml from the ZIP container.
pub fn parse_docx(path: &Path) -> Result<ParsedDoc> {
    let file = std::fs::File::open(path).map_err(|e| SearchError::Parse {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;

    let mut archive = zip::ZipArchive::new(file).map_err(|e| SearchError::Parse {
        path: path.to_path_buf(),
        reason: format!("ZIP error: {}", e),
    })?;

    let text = extract_document_xml(&mut archive, path)?;
    let title = extract_core_title(&mut archive);

    let title = title.or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
    });

    let snippet = text.chars().take(256).collect();

    Ok(ParsedDoc {
        text,
        title,
        snippet,
        metadata: StructuralMeta::default(),
    })
}

fn extract_document_xml<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    path: &Path,
) -> Result<String> {
    let mut xml_file = archive
        .by_name("word/document.xml")
        .map_err(|e| SearchError::Parse {
            path: path.to_path_buf(),
            reason: format!("word/document.xml not found: {}", e),
        })?;

    let mut xml_content = String::new();
    xml_file
        .read_to_string(&mut xml_content)
        .map_err(|e| SearchError::Parse {
            path: path.to_path_buf(),
            reason: format!("read error: {}", e),
        })?;

    Ok(xml_to_text(&xml_content))
}

/// Strip all XML tags, preserving text content with paragraph breaks.
fn xml_to_text(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut out = String::new();
    let mut in_paragraph = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let local = e.local_name();
                let name = std::str::from_utf8(local.as_ref()).unwrap_or("");
                if name == "p" {
                    in_paragraph = true;
                }
            }
            Ok(Event::End(ref e)) => {
                let local = e.local_name();
                let name = std::str::from_utf8(local.as_ref()).unwrap_or("");
                if name == "p" && in_paragraph {
                    out.push('\n');
                    in_paragraph = false;
                }
            }
            Ok(Event::Text(e)) => {
                if let Ok(text) = e.unescape() {
                    out.push_str(&text);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }

    out.trim().to_string()
}

/// Extract document title from docProps/core.xml if present.
fn extract_core_title<R: Read + std::io::Seek>(archive: &mut zip::ZipArchive<R>) -> Option<String> {
    let mut core_file = archive.by_name("docProps/core.xml").ok()?;
    let mut content = String::new();
    core_file.read_to_string(&mut content).ok()?;

    let mut reader = Reader::from_str(&content);
    let mut in_title = false;
    let mut title = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let local = e.local_name();
                let name = std::str::from_utf8(local.as_ref()).unwrap_or("");
                if name == "title" {
                    in_title = true;
                }
            }
            Ok(Event::Text(e)) if in_title => {
                if let Ok(text) = e.unescape() {
                    let t = text.trim().to_string();
                    if !t.is_empty() {
                        title = Some(t);
                    }
                }
                in_title = false;
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {
                if in_title {
                    in_title = false;
                }
            }
        }
    }

    title
}
