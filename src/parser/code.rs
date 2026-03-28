use std::path::Path;

use crate::error::Result;
use crate::indexer::schema::StructuralMeta;
use crate::parser::ParsedDoc;

/// Parse a source code file, stripping comments for cleaner indexing
/// while preserving identifiers, string literals, and structure.
pub fn parse_code(path: &Path, language: &str) -> Result<ParsedDoc> {
    let raw = std::fs::read_to_string(path).map_err(|e| crate::error::SearchError::Parse {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;

    let text = match language {
        "python" | "ruby" | "shell" => strip_hash_comments(&raw),
        "rust" | "go" | "javascript" | "typescript" | "java" | "c" | "cpp" | "kotlin"
        | "swift" => strip_c_style_comments(&raw),
        _ => raw.clone(),
    };

    let title = extract_code_title(path);
    let snippet = text.chars().take(256).collect();

    Ok(ParsedDoc {
        text,
        title,
        snippet,
        metadata: StructuralMeta::default(),
    })
}

/// Strip `#`-prefixed line comments (Python, Ruby, Shell).
fn strip_hash_comments(src: &str) -> String {
    src.lines()
        .map(|line| {
            // Don't strip shebangs
            if line.starts_with("#!") {
                return line.to_string();
            }
            // Find # outside of string literals (simplified: strip from # onwards)
            if let Some(pos) = find_comment_start_hash(line) {
                line[..pos].trim_end().to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Strip `//` line comments and `/* */` block comments (C-family).
fn strip_c_style_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    let mut in_block_comment = false;
    let mut in_line_comment = false;
    let mut in_string: Option<char> = None;

    while let Some(c) = chars.next() {
        if in_line_comment {
            if c == '\n' {
                in_line_comment = false;
                out.push('\n');
            }
            continue;
        }

        if in_block_comment {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block_comment = false;
            }
            continue;
        }

        if let Some(quote) = in_string {
            out.push(c);
            if c == '\\' {
                // Escaped character — consume next
                if let Some(escaped) = chars.next() {
                    out.push(escaped);
                }
            } else if c == quote {
                in_string = None;
            }
            continue;
        }

        match c {
            '"' | '\'' => {
                in_string = Some(c);
                out.push(c);
            }
            '/' => match chars.peek() {
                Some('/') => {
                    chars.next();
                    in_line_comment = true;
                }
                Some('*') => {
                    chars.next();
                    in_block_comment = true;
                }
                _ => out.push(c),
            },
            _ => out.push(c),
        }
    }

    out
}

/// Find the start of a `#` comment, ignoring `#` inside strings.
fn find_comment_start_hash(line: &str) -> Option<usize> {
    let mut in_string: Option<char> = None;
    for (i, c) in line.char_indices() {
        match in_string {
            Some(_q) if c == '\\' => {}
            Some(q) if c == q => in_string = None,
            Some(_) => {}
            None => match c {
                '"' | '\'' => in_string = Some(c),
                '#' => return Some(i),
                _ => {}
            },
        }
    }
    None
}

/// Use filename stem as the document title for code files.
fn extract_code_title(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_c_comments() {
        let src = r#"
fn main() {
    // This is a comment
    let x = 5; /* inline */ let y = 6;
    println!("hello // not a comment");
}
"#;
        let result = strip_c_style_comments(src);
        assert!(!result.contains("This is a comment"));
        assert!(!result.contains("inline"));
        assert!(result.contains("hello // not a comment")); // inside string
        assert!(result.contains("let x = 5;"));
    }

    #[test]
    fn test_strip_hash_comments() {
        let src = "x = 1  # a comment\ny = 2";
        let result = strip_hash_comments(src);
        assert!(!result.contains("a comment"));
        assert!(result.contains("x = 1"));
        assert!(result.contains("y = 2"));
    }
}
