//! Proof that rlex's union-default-graph behavior fixes the silent-empty trap.
//!
//! forx files every repo's quads under a named graph IRI, so nothing lives in
//! the store's bare default graph. A query without a GRAPH wrapper therefore
//! matches nothing and returns zero rows with no error. rlex's `build_query`
//! (mirrored here) unions the named graphs into the default graph unless the
//! query names its own dataset, so the natural query just works.
//!
//! Run against a real forx sample:
//!   cargo run --example union_default_proof -- \
//!     /tmp/repolex_sample/hukkin--tomli-w/aggregate/ast/<sha>.nq.gz
//!
//! With no argument it picks the largest .nq.gz under the tomli-w AST aggregate.

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use oxigraph::io::RdfFormat;
use oxigraph::model::Term;
use oxigraph::sparql::{Query, QueryResults};
use oxigraph::store::Store;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

const COUNT_ALL: &str = "SELECT (COUNT(*) AS ?n) WHERE { ?s ?p ?o }";

fn main() -> Result<()> {
    let path = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(default_sample);
    println!("Loading forx sample: {}", path.display());

    let store = Store::new()?;
    let file = File::open(&path).with_context(|| format!("opening {}", path.display()))?;
    let gz = GzDecoder::new(BufReader::new(file));
    let mut loader = store.bulk_loader();
    loader.load_from_reader(RdfFormat::NQuads, gz)?;
    loader.commit()?;
    println!("Store holds {} quads (all in named graphs)\n", store.len()?);

    // 1. The trap: a bare query against the default graph finds nothing.
    let bare = count(&store, store.query(COUNT_ALL)?)?;
    println!("  bare   `{{ ?s ?p ?o }}`                 -> {} rows", bare);

    // 2. The fix: union the named graphs into the default graph (what
    //    rlex::query::build_query does when the query has no FROM of its own).
    let mut q = Query::parse(COUNT_ALL, None)?;
    q.dataset_mut().set_default_graph_as_union();
    let unioned = count(&store, store.query(q)?)?;
    println!("  union  `{{ ?s ?p ?o }}`                 -> {} rows", unioned);

    // 3. Sanity: the explicit GRAPH wrapper agrees with the union count.
    let wrapped = count(
        &store,
        store.query("SELECT (COUNT(*) AS ?n) WHERE { GRAPH ?g { ?s ?p ?o } }")?,
    )?;
    println!("  GRAPH  `{{ GRAPH ?g {{ ?s ?p ?o }} }}`     -> {} rows", wrapped);

    println!();
    assert_eq!(bare, 0, "expected the bare default graph to be empty");
    assert!(unioned > 0, "expected the union to find the data");
    assert_eq!(unioned, wrapped, "union count should match the explicit GRAPH count");
    println!("PROVED: bare default graph is empty; union == GRAPH == {} rows.", unioned);
    println!("That is the difference between an agent getting nothing and getting the graph.");
    Ok(())
}

fn count(_store: &Store, results: QueryResults) -> Result<u64> {
    if let QueryResults::Solutions(mut sols) = results {
        if let Some(sol) = sols.next() {
            let sol = sol?;
            if let Some(Term::Literal(lit)) = sol.get("n") {
                return Ok(lit.value().parse().unwrap_or(0));
            }
        }
    }
    Ok(0)
}

fn default_sample() -> PathBuf {
    let dir = PathBuf::from("/tmp/repolex_sample/hukkin--tomli-w/aggregate/ast");
    std::fs::read_dir(&dir)
        .ok()
        .and_then(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.to_string_lossy().ends_with(".nq.gz"))
                .max_by_key(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
        })
        .unwrap_or(dir)
}
