#!/usr/bin/env bash
# doc-search (ds) — manual test commands
# Run from the repo root: bash test_commands.sh
# Assumes: cargo build --release already done

DS=./target/release/ds
FIXTURES=./tests/fixtures

echo "=== INDEX ==="
# Index fixtures folder (incremental)
$DS index $FIXTURES

# Force full re-index
$DS index $FIXTURES --full

echo ""
echo "=== SEARCH ==="
# Basic keyword search
$DS search "ranjan"

# Limit results
$DS search "ranjan" -n 3

# Larger context snippet (good for RAG)
$DS search "ranjan" --context-size 500

# Full file content in output
$DS search "ranjan" --full-content

# JSON output
$DS search "ranjan" --output json

# TSV output
$DS search "ranjan" --output tsv

# JSON + full content (RAG-ready)
$DS search "ranjan" --full-content --output json

# Multi-word / partial match
$DS search "agreement"

echo ""
echo "=== QUERY (DSL) ==="
# Filter by doc type
$DS query 'type:contract'

# Filter by type + keyword
$DS query 'type:contract "purchase"'

# Numeric amount filter
$DS query 'amount:>1000'

# Date filter
$DS query 'date:>2024-01-01'

# Path filter
$DS query 'path:/fixtures'

# Combined
$DS query 'type:pdf AND date:>2024-01-01'

# JSON output
$DS query 'type:contract' --output json

echo ""
echo "=== SIMILAR ==="
# Find documents similar to a fixture file
$DS similar $FIXTURES/sample.txt

# With hamming distance threshold
$DS similar $FIXTURES/sample.txt --threshold 10

# Limit results
$DS similar $FIXTURES/sample.txt -n 5

# JSON output
$DS similar $FIXTURES/sample.txt --output json

echo ""
echo "=== RECENT ==="
# Last 7 days (default)
$DS recent

# Last 30 days
$DS recent --since 30d

# Since a specific date
$DS recent --since 2025-01-01

# Weeks / months
$DS recent --since 2w
$DS recent --since 3m

# Limit results
$DS recent --since 30d -n 5

# JSON output
$DS recent --output json

echo ""
echo "=== CLUSTERS ==="
# Cluster all indexed docs (coarse)
$DS clusters

# Fine-grained clustering
$DS clusters --bits 8

# Restrict to a sub-path
$DS clusters $FIXTURES

# JSON output
$DS clusters --output json

echo ""
echo "=== GLOBAL FLAGS ==="
# Custom index location
$DS --index /tmp/test_index index $FIXTURES
$DS --index /tmp/test_index search "ranjan"

echo ""
echo "Done."
