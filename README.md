# contextgrep

Grep your documents with context. Fast offline search for PDFs, DOCX, Markdown, and code — no vectors, no cloud, no ML dependencies. Trigram indexing + SimHash fingerprinting built in Rust.

```
scout index ./docs/
scout search "purchase agreement"       # grep with context
scout query 'type:contract amount:>1M'  # structured DSL
scout similar ./contract_draft.docx    # find near-duplicates
scout recent --since 7d
scout clusters ./docs/
```

---

## Quick Start

**1. Install the MCP server** (one-time)

```bash
claude mcp add scout npx contextgrep@latest
```

> macOS Homebrew users: use `$(which npx)` instead of `npx`

**2. Index your documents** (one-time per folder)

Just tell your AI assistant:

> *"Index my documents at /Users/john/Documents/contracts"*

The assistant calls the `index` tool automatically. The index is saved to `.searchindex/` inside that folder — subsequent runs are incremental and only process new or changed files.

**3. Start searching**

> *"Find all contracts mentioning indemnification"*
> *"Which documents have a purchase price over $1M?"*
> *"Show me files modified in the last 7 days"*
> *"Find documents similar to this NDA"*

The assistant picks the right tool (`search`, `query`, `recent`, `similar`) based on your question.

---

## Why not vectors or BM25?

| | This tool | Vector search | BM25 |
|---|---|---|---|
| Works offline | yes | no (needs model) | yes |
| Deterministic | yes | no | yes |
| Handles typos/partials | yes | sometimes | no |
| Cost | zero | $$$ (inference) | zero |
| Explainable results | yes | no | partially |
| Finds near-duplicates | yes | yes | no |

The goal: something that feels as fast as `grep`, understands document structure, and needs zero infrastructure.

---

## How it works

Search runs in 3 stages:

```
Query
  ↓
Trigram index       — fast fuzzy/substring candidate retrieval
  ↓
Structural filter   — hard constraints (type:, path:, amount:, date:)
  ↓
Scoring             — trigram overlap + term proximity + recency + structure + title boost
```

**Trigram index** — splits text into overlapping 3-character windows and builds posting lists. Handles typos, partial matches, and substring queries without needing exact word boundaries.

**SimHash** — computes a 64-bit document fingerprint from word bigram shingles. Two documents with fewer than ~8 differing bits are near-duplicates. Used by `scout similar` and `scout clusters`.

**Structural metadata** — regex-based extraction of dates, currency amounts, email addresses, and document type inference. Used by the DSL filter layer.

**Scoring formula:**
```
score = 0.45 × trigram_overlap
      + 0.20 × term_proximity
      + 0.10 × recency_decay
      + 0.20 × structural_field_match
      + 0.05 × title_boost
```

---

## Installation

### MCP server (Claude Code, Claude Desktop, Cursor, VS Code…)

No Rust required. Add this to your MCP config:

```json
{
  "mcpServers": {
    "scout": {
      "command": "npx",
      "args": ["contextgrep@latest"]
    }
  }
}
```

Or via Claude Code CLI:

```bash
claude mcp add scout npx contextgrep@latest
```

> **macOS (Homebrew Node.js):** If the server fails to connect, use the full path to `npx`:
> ```bash
> claude mcp add scout $(which npx) contextgrep@latest
> ```

With a custom index location:

```json
{
  "mcpServers": {
    "scout": {
      "command": "npx",
      "args": ["contextgrep@latest", "--index", "/path/to/your/index"]
    }
  }
}
```

**Config file locations:**

| Client | Config file |
|---|---|
| Claude Code | `~/.claude/settings.json` |
| Claude Desktop (macOS) | `~/Library/Application Support/Claude/claude_desktop_config.json` |
| Cursor | `~/.cursor/mcp.json` or `.cursor/mcp.json` in project |
| VS Code | `.vscode/mcp.json` |
| Zed | `~/.config/zed/settings.json` → `context_servers` |

---

### CLI binary

**Download a pre-built binary** from [GitHub Releases](https://github.com/ranjanj1/scout/releases), put it in your PATH, then:

```bash
scout index ./docs/
scout search "purchase agreement"
```

**Build from source** (requires Rust 1.75+):

```bash
git clone https://github.com/ranjanj1/scout
cd scout
cargo install --path .
```

---

## Supported file types

| Type | Extensions |
|---|---|
| Plain text | `.txt`, `.text` |
| Markdown | `.md`, `.markdown` |
| PDF | `.pdf` |
| Word documents | `.docx` |
| Config/data | `.toml`, `.yaml`, `.yml`, `.json` |
| Code | `.rs`, `.py`, `.js`, `.ts`, `.go`, `.java`, `.c`, `.cpp`, `.rb`, `.swift`, `.kt`, `.sh` |

Respects `.gitignore`, `.ignore`, and `.searchignore` files during walks.

---

## Commands

### `scout index <path>`

Index a folder. Subsequent runs are incremental — only new or changed files are re-indexed.

```bash
scout index ./docs/
scout index ./docs/ --full        # force full re-index
```

### `scout search <query>`

Trigram-based fuzzy search. Handles partial words and minor typos.

```bash
scout search "purchase agreement"
scout search "purchse agreem"           # typo-tolerant
scout search "2024 contract"
scout search "indemnif"                 # prefix match
scout search "agreement" -n 5           # top 5 results
```

**Snippet control:**

`--context-size N` (default 120) extracts N characters on **both sides** of the match, so the hit sits in the middle of the snippet (~2×N chars total).

```bash
scout search "indemnif" --context-size 500    # ~1000 chars around each match
scout search "indemnif" --full-content        # return entire file text instead of a snippet
```

**RAG usage** — pipe full content as JSON into your LLM:

```bash
scout search "purchase price" --full-content --output json -n 3
```

```json
[
  {
    "path": "./docs/acquisition.pdf",
    "score": 0.72,
    "type": "contract",
    "snippet": "...the purchase price shall be...",
    "content": "Agreement for Services\n\nThis agreement..."
  }
]
```

```python
import subprocess, json

def retrieve(question: str, top_k: int = 3) -> list[dict]:
    result = subprocess.run(
        ["scout", "search", question, "--full-content", "--output", "json", "-n", str(top_k)],
        capture_output=True, text=True,
    )
    return json.loads(result.stdout)
```

### `scout query <dsl>`

Structured search using the filter DSL.

```bash
scout query 'type:contract'
scout query 'amount:>1M'
scout query 'type:contract amount:>500K path:/legal'
scout query '"non-disclosure" AND date:>2024-01-01'
scout query 'type:invoice OR type:receipt'
scout query 'NOT type:draft'
```

**DSL reference:**

| Syntax | Meaning |
|---|---|
| `type:contract` | Document type equals (fuzzy) |
| `amount:>1M` | Any extracted amount > 1,000,000 |
| `amount:<=50000` | Any extracted amount ≤ 50,000 |
| `date:>2024-01-01` | Any extracted date after Jan 1 2024 |
| `date:>2024` | Any extracted date after 2024 |
| `path:/legal` | File path contains `/legal` |
| `email:@acme.com` | Email matching `@acme.com` found in doc |
| `"exact phrase"` | Phrase must appear as written |
| `AND`, `OR`, `NOT` | Boolean operators |
| `(...)` | Grouping |

### `scout similar <file>`

Find documents similar to a given file using SimHash Hamming distance.

```bash
scout similar ./contract_v1.docx
scout similar ./contract_v1.docx --threshold 12    # looser matching
scout similar ./contract_v1.docx -n 20             # top 20 results
```

Similarity score is `1 - (hamming_distance / 64)`. Score of 1.0 = identical content, 0.875 = 8 bits differ.

### `scout recent`

Show recently modified documents, sorted newest first.

```bash
scout recent --since 7d      # last 7 days
scout recent --since 2w      # last 2 weeks
scout recent --since 3m      # last 3 months
scout recent --since 1y      # last year
scout recent --since 2024-06-01   # since a specific date
```

### `scout clusters`

Group documents into similarity clusters using Locality-Sensitive Hashing on SimHash fingerprints.

```bash
scout clusters ./docs/
scout clusters ./docs/ --bits 4    # coarse clustering (fewer, larger groups)
scout clusters ./docs/ --bits 8    # fine-grained clustering
```

---

## Output formats

All commands support `--output plain|json|tsv`:

```bash
# Default: human-readable
scout search "agreement"

# JSON: newline-delimited, good for scripting
scout search "agreement" --output json | jq '.[0].path'

# TSV: tab-separated path, score, snippet
scout search "agreement" --output tsv | cut -f1
```

---

## Index location

The index is stored in `.searchindex/` and resolved with this precedence:

1. `--index <path>` CLI flag
2. `.searchindex/` in the current directory or any parent (walks up)
3. `~/.searchindex/` as a global fallback

```
.searchindex/
  segments/
    0000/
      postings.trgm    # trigram posting lists (mmap binary)
      simhash.bin      # flat u64 array of SimHash fingerprints
    0001/              # incremental segment from next run
  docstore.redb        # document metadata and snippets
  metadata.redb        # structural metadata (dates, amounts, etc.)
```

Incremental indexing: each `scout index` run compares `mtime` and `xxh64` file hash. Unchanged files are skipped. Deleted files are removed from the index. Segments merge automatically when count exceeds 8.

To exclude files from indexing, add patterns to `.searchignore` (same syntax as `.gitignore`).

---

## Development

```bash
# Run all tests
cargo test

# Run specific test file
cargo test --test index_test

# Run unit tests for a module
cargo test trigram
cargo test simhash
cargo test query

# Run benchmarks
cargo bench

# Build release binary
cargo build --release
./target/release/scout --help
```

**Project layout:**

```
src/
  cli/          commands.rs, output.rs
  parser/       text, pdf, docx, code, walker, metadata
  indexer/      trigram, simhash, schema, pipeline
  search/       query DSL, filters, proximity, scorer
  storage/      mmap (postings), redb store, segment manager
  config.rs     index path resolution
  error.rs      unified error type
tests/
  fixtures/     sample.txt, sample.md, sample.rs
  integration/  index_test, search_test, query_test
benches/        trigram, simhash, search benchmarks
```

---

## Limitations

- **Semantic queries will miss results.** Searching for "indemnification" won't match a document that only says "liability protection". Trigrams are lexical, not conceptual.
- **PDF quality varies.** Image-based PDFs (scanned documents) produce no text.
- **No ranking feedback loop.** Scoring weights are fixed defaults; there's no click-through learning.
- **Single-machine only.** No distributed index, no server mode.

---

## License

MIT
