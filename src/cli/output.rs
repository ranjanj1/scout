use crate::cli::commands::OutputFormat;

/// A single search result to be rendered.
pub struct SearchResult {
    pub path: String,
    pub score: f64,
    pub snippet: String,
    pub doc_type: Option<String>,
    /// Full or partial document content for RAG use (None = not requested)
    pub content: Option<String>,
}

pub fn render_results(results: &[SearchResult], format: &OutputFormat) {
    match format {
        OutputFormat::Plain => render_plain(results),
        OutputFormat::Json => render_json(results),
        OutputFormat::Tsv => render_tsv(results),
    }
}

fn render_plain(results: &[SearchResult]) {
    if results.is_empty() {
        eprintln!("No results found.");
        return;
    }
    for (i, r) in results.iter().enumerate() {
        let kind = r.doc_type.as_deref().unwrap_or("doc");
        println!(
            "[{}] {} (score: {:.3}, type: {})",
            i + 1, r.path, r.score, kind
        );
        if let Some(content) = &r.content {
            println!("--- content ---");
            println!("{}", content.trim());
            println!("---------------");
        } else if !r.snippet.is_empty() {
            let snippet = r.snippet.lines().next().unwrap_or("").trim();
            println!("    {}", snippet);
        }
        println!();
    }
}

fn render_json(results: &[SearchResult]) {
    println!("[");
    for (i, r) in results.iter().enumerate() {
        let comma = if i + 1 < results.len() { "," } else { "" };
        println!(
            "  {{\"path\":{},\"score\":{:.6},\"type\":{},\"snippet\":{},\"content\":{}}}{}",
            json_str(&r.path),
            r.score,
            json_opt_str(r.doc_type.as_deref()),
            json_str(&r.snippet),
            json_opt_str(r.content.as_deref()),
            comma
        );
    }
    println!("]");
}

fn render_tsv(results: &[SearchResult]) {
    for r in results {
        let snippet_oneline = r.snippet.replace('\t', " ").replace('\n', " ");
        println!("{}\t{:.6}\t{}", r.path, r.score, snippet_oneline);
    }
}

fn json_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n"))
}

fn json_opt_str(s: Option<&str>) -> String {
    match s {
        Some(v) => json_str(v),
        None => "null".to_string(),
    }
}

pub fn render_error(msg: &str) {
    eprintln!("error: {}", msg);
}

pub fn render_info(msg: &str) {
    eprintln!("{}", msg);
}
