use std::path::PathBuf;

use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ServerHandler, ServiceExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::io::{stdin, stdout};

use scout::error::{Result as ScoutResult, SearchError};

/// Run the scout MCP server over stdio.
pub fn run_mcp_server(index_override: Option<PathBuf>) -> ScoutResult<()> {
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| SearchError::Mmap(e.to_string()))?;

    rt.block_on(async {
        let server = ScoutMcp::new(index_override);
        let service = server
            .serve((stdin(), stdout()))
            .await
            .map_err(|e| SearchError::Mmap(e.to_string()))?;
        service
            .waiting()
            .await
            .map_err(|e| SearchError::Mmap(e.to_string()))?;
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Server struct
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct ScoutMcp {
    binary: String,
    index: Option<PathBuf>,
    tool_router: ToolRouter<Self>,
}

impl ScoutMcp {
    fn new(index: Option<PathBuf>) -> Self {
        let binary = std::env::current_exe()
            .ok()
            .and_then(|p| p.to_str().map(|s| s.to_owned()))
            .unwrap_or_else(|| "scout".to_owned());
        Self {
            binary,
            index,
            tool_router: Self::tool_router(),
        }
    }

    fn cmd(&self, subcommand: &str) -> std::process::Command {
        let mut c = std::process::Command::new(&self.binary);
        if let Some(idx) = &self.index {
            c.arg("--index").arg(idx);
        }
        c.arg(subcommand);
        c
    }

    fn run(&self, mut cmd: std::process::Command) -> String {
        match cmd.output() {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
                let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
                if !out.status.success() && stdout.is_empty() {
                    format!("error: {}", stderr.trim())
                } else {
                    stdout
                }
            }
            Err(e) => format!("error: failed to run scout: {e}"),
        }
    }
}

// ---------------------------------------------------------------------------
// ServerHandler
// ---------------------------------------------------------------------------

#[tool_handler(router = self.tool_router)]
impl ServerHandler for ScoutMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .build(),
        )
    }
}

// ---------------------------------------------------------------------------
// Tool input schemas
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct IndexInput {
    /// Path to the folder to index.
    path: String,
    /// Force a full re-index (ignore change detection).
    #[serde(default)]
    full: bool,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct SearchInput {
    /// Search query — supports partial words and minor typos.
    query: String,
    /// Maximum number of results (default 10).
    #[serde(default = "default_limit")]
    limit: usize,
    /// Characters of context on each side of the match (default 120, try 500 for RAG).
    #[serde(default = "default_context")]
    context_size: usize,
    /// Return full file content instead of a snippet (useful for RAG).
    #[serde(default)]
    full_content: bool,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct QueryInput {
    /// DSL query. Examples: `type:contract`, `amount:>1M`, `type:pdf AND date:>2024-01-01`, `path:/legal`
    dsl: String,
    /// Maximum number of results (default 10).
    #[serde(default = "default_limit")]
    limit: usize,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct SimilarInput {
    /// Path to the reference file.
    file: String,
    /// Max Hamming distance (0–64). Omit to return top-N closest regardless of distance.
    threshold: Option<u32>,
    /// Maximum number of results (default 10).
    #[serde(default = "default_limit")]
    limit: usize,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct RecentInput {
    /// Time window: "7d", "2w", "3m", or "2026-01-01" (default "7d").
    #[serde(default = "default_since")]
    since: String,
    /// Maximum number of results (default 20).
    #[serde(default = "default_recent_limit")]
    limit: usize,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ClustersInput {
    /// Restrict clustering to this sub-path (optional).
    path: Option<String>,
    /// LSH band bits: 4 = coarse, 8 = fine-grained (default 4).
    #[serde(default = "default_bits")]
    bits: u32,
}

fn default_limit() -> usize { 10 }
fn default_context() -> usize { 120 }
fn default_since() -> String { "7d".into() }
fn default_recent_limit() -> usize { 20 }
fn default_bits() -> u32 { 4 }

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

#[tool_router(router = tool_router)]
impl ScoutMcp {
    #[tool(description = "Index a folder. Must be run before search/query/similar/recent/clusters.")]
    async fn index(&self, Parameters(input): Parameters<IndexInput>) -> String {
        let mut cmd = self.cmd("index");
        cmd.arg(&input.path);
        if input.full {
            cmd.arg("--full");
        }
        self.run(cmd)
    }

    #[tool(description = "Trigram fuzzy search. Returns scored JSON results with snippets or full content.")]
    async fn search(&self, Parameters(input): Parameters<SearchInput>) -> String {
        let mut cmd = self.cmd("search");
        cmd.arg(&input.query)
            .arg("-n").arg(input.limit.to_string())
            .arg("--context-size").arg(input.context_size.to_string())
            .arg("--output").arg("json");
        if input.full_content {
            cmd.arg("--full-content");
        }
        self.run(cmd)
    }

    #[tool(description = "Structured DSL search. Filters: type:contract, amount:>1M, date:>2024-01-01, path:/legal, AND/OR/NOT.")]
    async fn query(&self, Parameters(input): Parameters<QueryInput>) -> String {
        let mut cmd = self.cmd("query");
        cmd.arg(&input.dsl)
            .arg("-n").arg(input.limit.to_string())
            .arg("--output").arg("json");
        self.run(cmd)
    }

    #[tool(description = "Find documents similar to a given file using SimHash fingerprinting.")]
    async fn similar(&self, Parameters(input): Parameters<SimilarInput>) -> String {
        let mut cmd = self.cmd("similar");
        cmd.arg(&input.file)
            .arg("-n").arg(input.limit.to_string())
            .arg("--output").arg("json");
        if let Some(t) = input.threshold {
            cmd.arg("--threshold").arg(t.to_string());
        }
        self.run(cmd)
    }

    #[tool(description = "List recently modified indexed documents within a time window.")]
    async fn recent(&self, Parameters(input): Parameters<RecentInput>) -> String {
        let mut cmd = self.cmd("recent");
        cmd.arg("--since").arg(&input.since)
            .arg("-n").arg(input.limit.to_string())
            .arg("--output").arg("json");
        self.run(cmd)
    }

    #[tool(description = "Group indexed documents into similarity clusters using SimHash LSH.")]
    async fn clusters(&self, Parameters(input): Parameters<ClustersInput>) -> String {
        let mut cmd = self.cmd("clusters");
        cmd.arg("--bits").arg(input.bits.to_string())
            .arg("--output").arg("json");
        if let Some(p) = &input.path {
            cmd.arg(p);
        }
        self.run(cmd)
    }
}
