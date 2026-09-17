# rlex

> High-performance SPARQL query engine, catalog, and visualization server for [Repolex](https://repolex.ai) and Forx code knowledge graphs, written in Rust.

`rlex` is the compiled, high-throughput Rust successor to Python `lexq`. Built on top of [OxiGraph](https://github.com/oxigraph/oxigraph), `rlex` allows developers and autonomous AI agents to query massive repository knowledge graphs (ASTs, CFGs, DFGs, call graphs, symbol definitions) using standard SPARQL at native speed.

---

## Key Features

- **Blazing Fast SPARQL Execution**: Powered by OxiGraph 0.5 in-memory and persistent stores.
- **Transparent Graph Unioning**: Default queries evaluate against the union of all named graphs, ensuring cross-repository queries work without verbose `GRAPH` clauses (with `--no-union` option for strict SPARQL).
- **Forx Index Integration**: Syncs seamlessly with `forx-index`, discovering pre-parsed repositories, commits, and tags.
- **Multiple Output Formats**: Supports `table` (pretty terminal output), `json`, `tsv`, `csv`, `turtle`, `ntriples`, and `json-ld`.
- **Integrated HTTP & Viz Server**: Built-in Axum/Actix web services providing SPARQL HTTP endpoints (`:7878`) and browser-based graph visualization (`:3000`).

---

## Installation

### Prerequisites
- Rust toolchain (Edition 2024 / Rust 1.85+)
- Cargo

### Building from Source

```bash
# Clone the repository
git clone https://github.com/repolex-ai/rlex.git
cd rlex

# Build release binaries (rlex CLI and rlex-viz)
cargo build --release

# Install globally to your cargo bin path (~/.cargo/bin)
cargo install --path crates/rlex
```

Verify installation:

```bash
rlex --help
```

---

## Quickstart & Common Workflows

### 1. Sync the Repository Catalog
Synchronize local metadata with the latest `forx-index`:

```bash
rlex sync
```

### 2. Search & Inspect Available Repositories
Search for repositories in the index or view commit parse status:

```bash
# Search for repositories matching a keyword
rlex repos requests

# Inspect a specific repository (e.g. psf/requests)
rlex repos psf/requests
```

### 3. Download & Load Graphs
Fetch pre-parsed semantic graphs and ingest them into the local OxiGraph store:

```bash
# Download pre-parsed graph bundle for a repo commit
rlex download psf/requests v2.31.0

# View downloaded cache
rlex cache

# Load cached graph files into the local database
rlex load psf/requests
```

### 4. Query with SPARQL
Run SPARQL queries directly from the command line:

```bash
# Find all function definitions and their containing files
rlex query '
PREFIX code: <https://repolex.ai/ontology/code/>
SELECT ?fn ?name ?file WHERE {
  ?fn a code:Function ;
      code:name ?name ;
      code:file ?file .
} LIMIT 10
'

# Output as JSON for scripting or agent consumption
rlex query 'SELECT * WHERE { ?s ?p ?o } LIMIT 5' --format json
```

Supported formats: `table` (default), `json`, `csv`, `tsv`, `turtle`, `ntriples`, `json-ld`.

### 5. Interactive Web Server & Visualization
Start the embedded SPARQL HTTP endpoint and visualization UI:

```bash
# Start SPARQL endpoint on port 7878 and web UI on port 3000
rlex viz

# Or start just the SPARQL HTTP server in foreground mode
rlex serve --port 7878 --foreground
```

---

## CLI Command Reference

| Command | Arguments / Flags | Description |
|---|---|---|
| `rlex sync` | None | Clones or pulls the latest `forx-index` catalog. |
| `rlex repos` | `[query]` | Searches indexed repositories or inspects commit statuses. |
| `rlex download` | `<repo> <target> [-g graphs]` | Downloads pre-parsed graph archives for a tag or commit SHA. |
| `rlex cache` | None | Displays locally cached graph archives and disk usage. |
| `rlex load` | `<repo> [commit]` | Loads cached graphs into the local OxiGraph database. |
| `rlex query` | `<sparql> [-f format] [--no-union]` | Runs a SPARQL query against the database. |
| `rlex serve` | `[-p port] [--foreground] [--stop]` | Runs the SPARQL HTTP endpoint and catalog API. |
| `rlex viz` | `[-p port] [--sparql-port port] [--stop]` | Launches the visualization web UI and SPARQL service. |
| `rlex config` | None | Displays current configuration paths and directory settings. |

---

## Workspace Structure

- [`crates/rlex`](crates/rlex) — The primary CLI binary, catalog manager, cache loader, and query engine.
- [`crates/rlex-viz`](crates/rlex-viz) — Lightweight embedded web service hosting the graph visualization interface.

---

## Ecosystem Integration

`rlex` operates within the Repolex code intelligence stack:
1. **[repolex](https://github.com/repolex-ai/repolex-parser-py)** parses source code AST, CFG, DFG, and Call Graphs.
2. **[forx](https://github.com/repolex-ai/forx)** indexes repositories and generates serialized RDF graph dumps.
3. **`rlex`** loads and queries the compiled graphs at native speed, exposing SPARQL endpoints for human developers and autonomous agents.

---

## License

Dual License: AGPL-3.0-or-later or Commercial. Contact [Repolex](https://repolex.ai) for commercial licensing options.
