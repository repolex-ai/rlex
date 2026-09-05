//! Proof of priority #1: many repos in one store, cross-repo queryable.
//!
//! forx stamps every quad's named graph IRI with the repo it came from
//! (`https://repolex.ai/r/{org}/{repo}/{kind}/{sha}`), so loading several repos
//! into one store namespaces them with no collision — repo A's data can never be
//! mistaken for repo B's, yet one query with the union default (see
//! union_default_proof.rs) can range over all of them at once. That is the
//! foundation the cross-repo call-graph join stands on.
//!
//! Run:
//!   cargo run --example multi_repo_join
//!   cargo run --example multi_repo_join -- /path/a.nq.gz /path/b.nq.gz ...
//!
//! With no args it loads every repo it finds under /tmp/repolex_sample.

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use oxigraph::io::RdfFormat;
use oxigraph::model::Term;
use oxigraph::sparql::{Query, QueryResults};
use oxigraph::store::Store;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

fn main() -> Result<()> {
    let files: Vec<PathBuf> = {
        let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
        if args.is_empty() { discover_samples() } else { args }
    };
    if files.is_empty() {
        anyhow::bail!("no .nq.gz files given and none found under /tmp/repolex_sample");
    }

    let store = Store::new()?;
    println!("Loading {} AST file(s) from separate repos into ONE store:", files.len());
    for path in &files {
        let before = store.len()?;
        let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
        let mut loader = store.bulk_loader();
        loader.load_from_reader(RdfFormat::NQuads, GzDecoder::new(BufReader::new(file)))?;
        loader.commit()?;
        println!("  +{:>7} quads  {}", store.len()? - before, short(path));
    }
    println!("\nStore holds {} quads total.\n", store.len()?);

    // One union query ranges over every repo at once, split back out by the
    // repo baked into each named graph IRI. Distinct rows == clean namespacing.
    let by_repo = r#"
        SELECT ?repo (COUNT(*) AS ?quads) (COUNT(DISTINCT ?g) AS ?graphs) WHERE {
            GRAPH ?g { ?s ?p ?o }
            BIND(REPLACE(STR(?g), "^https://repolex.ai/r/([^/]+/[^/]+)/.*$", "$1") AS ?repo)
        }
        GROUP BY ?repo
        ORDER BY DESC(?quads)
    "#;

    println!("One query, grouped by the repo inside each graph IRI:");
    println!("  {:<28} {:>10} {:>8}", "repo", "quads", "graphs");
    println!("  {:-<28} {:->10} {:->8}", "", "", "");
    let mut repos = 0;
    if let QueryResults::Solutions(sols) = store.query(unioned(by_repo)?)? {
        for sol in sols {
            let sol = sol?;
            let repo = lit(sol.get("repo"));
            let quads = lit(sol.get("quads"));
            let graphs = lit(sol.get("graphs"));
            println!("  {:<28} {:>10} {:>8}", repo, quads, graphs);
            repos += 1;
        }
    }

    println!("\n{} repos coexist in one store, each cleanly separated, all reachable", repos);
    println!("from a single query. That is the store the cross-repo call join runs on.");
    Ok(())
}

/// Parse a query and make the default graph the union of all named graphs —
/// the same behavior rlex ships (see rlex::query::build_query).
fn unioned(sparql: &str) -> Result<Query> {
    let mut q = Query::parse(sparql, None)?;
    q.dataset_mut().set_default_graph_as_union();
    Ok(q)
}

fn lit(t: Option<&Term>) -> String {
    match t {
        Some(Term::Literal(l)) => l.value().to_string(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

fn short(p: &PathBuf) -> String {
    let s = p.to_string_lossy();
    s.strip_prefix("/tmp/repolex_sample/").map(|x| x.to_string()).unwrap_or_else(|| s.to_string())
}

/// One AST file per repo found under /tmp/repolex_sample.
fn discover_samples() -> Vec<PathBuf> {
    let root = PathBuf::from("/tmp/repolex_sample");
    let mut out = Vec::new();
    let Ok(repos) = std::fs::read_dir(&root) else { return out };
    for repo in repos.filter_map(|e| e.ok()) {
        let ast = repo.path().join("aggregate/ast");
        if let Some(f) = first_nqgz(&ast) {
            out.push(f);
        }
    }
    out.sort();
    out
}

/// Deepest-first search for one .nq.gz under a dir (handles both the flat
/// `<sha>.nq.gz` and chunked `<sha>/chunk-001.nq.gz` shapes).
fn first_nqgz(dir: &PathBuf) -> Option<PathBuf> {
    let mut found = None;
    for entry in walkdir::WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() && entry.path().to_string_lossy().ends_with(".nq.gz") {
            found = Some(entry.path().to_path_buf());
            break;
        }
    }
    found
}
