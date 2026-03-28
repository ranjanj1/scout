use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "ds",
    version,
    about = "Fast offline document search — trigram + SimHash + structure",
    long_about = None
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Output format
    #[arg(long, global = true, default_value = "plain", value_enum)]
    pub output: OutputFormat,

    /// Override index location (default: .searchindex/ in cwd or ancestors)
    #[arg(long, global = true)]
    pub index: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Index a folder (supports incremental updates)
    Index {
        /// Path to index
        path: PathBuf,

        /// Force full re-index, ignoring change detection
        #[arg(long)]
        full: bool,
    },

    /// Fuzzy substring search using the trigram index
    Search {
        /// Search query (supports partial terms, typos via trigram overlap)
        query: String,

        /// Maximum number of results
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,

        /// Characters of context around each match (for RAG use, try 1500)
        #[arg(long, default_value = "120")]
        context_size: usize,

        /// Include full file content in output instead of a snippet
        #[arg(long)]
        full_content: bool,
    },

    /// Structured query using the filter DSL
    ///
    /// Examples:
    ///   ds query 'type:contract amount:>1M'
    ///   ds query 'path:/legal "purchase agreement"'
    ///   ds query 'type:pdf AND date:>2024-01-01'
    Query {
        /// DSL query string
        dsl: String,

        /// Maximum number of results
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,
    },

    /// Find documents similar to a given file (via SimHash)
    Similar {
        /// Path to the reference file
        file: PathBuf,

        /// Max hamming distance threshold (0-64). Omit to return top-N closest regardless of distance.
        #[arg(long)]
        threshold: Option<u32>,

        /// Maximum number of results
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,
    },

    /// Show recently modified documents
    Recent {
        /// Time window (e.g. "7d", "2w", "3m", "2026-01-01")
        #[arg(long, default_value = "7d")]
        since: String,

        /// Maximum number of results
        #[arg(short = 'n', long, default_value = "20")]
        limit: usize,
    },

    /// Start an MCP server (stdio transport) exposing all search tools
    Mcp,

    /// Group indexed documents into clusters by SimHash similarity
    Clusters {
        /// Path to restrict clustering to (defaults to entire index)
        path: Option<PathBuf>,

        /// Number of LSH band bits (4 = coarse, 8 = fine-grained)
        #[arg(long, default_value = "4")]
        bits: u32,
    },
}

#[derive(Clone, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable with highlighted snippets
    Plain,
    /// Newline-delimited JSON for scripting
    Json,
    /// Tab-separated: path, score, snippet
    Tsv,
}
