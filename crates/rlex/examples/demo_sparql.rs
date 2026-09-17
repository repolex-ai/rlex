use anyhow::Result;
use flate2::read::GzDecoder;
use oxigraph::io::RdfFormat;
use oxigraph::sparql::QueryResults;
use oxigraph::store::Store;
use std::fs::File;
use std::io::BufReader;

fn main() -> Result<()> {
    println!("=== Initializing Oxigraph In-Memory Graph Store ===");
    let store = Store::new()?;

    let nq_path = "/tmp/repolex_sample/pydantic--pydantic/aggregate/lsp/cf67d4b3193c3fe43ede18612ed62785eee11382.nq.gz";
    println!("Loading Pydantic LSP Aggregate: {}", nq_path);

    let file = File::open(nq_path)?;
    let gz = GzDecoder::new(BufReader::new(file));
    let mut reader = BufReader::new(gz);

    store.load_from_reader(RdfFormat::NQuads, &mut reader)?;
    println!("✓ Graph loaded! Store contains {} quads.\n", store.len()?);

    // =========================================================================
    // Query 1: Top External Packages Resolved by MultiLSP
    // =========================================================================
    let q1 = r#"
        PREFIX lsp-x: <https://repolex.ai/ontology/repolex/lsp-extension/>

        SELECT ?pkg (COUNT(?call) AS ?callCount) WHERE {
            GRAPH ?g {
                ?call lsp-x:externalPackage ?pkg .
            }
        }
        GROUP BY ?pkg
        ORDER BY DESC(?callCount)
        LIMIT 10
    "#;

    println!("--------------------------------------------------------------------------------");
    println!("SPARQL Query 1: External Package Invocations Resolved by MultiLSP");
    println!("--------------------------------------------------------------------------------");
    if let QueryResults::Solutions(solutions) = store.query(q1)? {
        println!("{:<35} | {:<10}", "External Package", "Resolved Calls");
        println!("{:-<35}-+-{:-<10}", "", "");
        for sol in solutions {
            let sol = sol?;
            let pkg = sol.get("pkg").map(|t| t.to_string()).unwrap_or_default();
            let count = sol.get("callCount").map(|t| t.to_string()).unwrap_or_default();
            println!("{:<35} | {:<10}", pkg.trim_matches('"'), count.trim_matches('"'));
        }
    }

    // =========================================================================
    // Query 2: Cross-Language Python -> Rust Call Resolutions
    // =========================================================================
    let q2 = r#"
        PREFIX lsp-x: <https://repolex.ai/ontology/repolex/lsp-extension/>

        SELECT ?src ?tgt (COUNT(?call) AS ?calls) WHERE {
            GRAPH ?g {
                ?call lsp-x:resolutionSourceFile ?src ;
                      lsp-x:resolutionTargetFile ?tgt .
                FILTER (REGEX(?src, "\\.py$") && REGEX(?tgt, "pydantic-core|\\.rs$"))
            }
        }
        GROUP BY ?src ?tgt
        ORDER BY DESC(?calls)
        LIMIT 12
    "#;

    println!("\n--------------------------------------------------------------------------------");
    println!("SPARQL Query 2: Cross-Language Boundary Calls (Python -> Rust Core)");
    println!("--------------------------------------------------------------------------------");
    if let QueryResults::Solutions(solutions) = store.query(q2)? {
        println!("{:<42} | {:<38} | {:<8}", "Python Source File", "Rust Target File", "Calls");
        println!("{:-<42}-+-{:-<38}-+-{:-<8}", "", "", "");
        for sol in solutions {
            let sol = sol?;
            let src = sol.get("src").map(|t| t.to_string()).unwrap_or_default();
            let tgt = sol.get("tgt").map(|t| t.to_string()).unwrap_or_default();
            let count = sol.get("calls").map(|t| t.to_string()).unwrap_or_default();
            println!("{:<42} | {:<38} | {:<8}", src.trim_matches('"'), tgt.trim_matches('"'), count.trim_matches('"'));
        }
    }

    // =========================================================================
    // Query 3: PyO3 FFI Call Targets with Line Numbers
    // =========================================================================
    let q3 = r#"
        PREFIX lsp-x: <https://repolex.ai/ontology/repolex/lsp-extension/>

        SELECT ?call ?src ?line WHERE {
            GRAPH ?g {
                ?call lsp-x:externalPackage "pyo3" ;
                      lsp-x:resolutionSourceFile ?src ;
                      lsp-x:callTargetLine ?line .
            }
        }
        LIMIT 8
    "#;

    println!("\n--------------------------------------------------------------------------------");
    println!("SPARQL Query 3: Specific PyO3 Rust FFI Call Targets & Line Numbers");
    println!("--------------------------------------------------------------------------------");
    if let QueryResults::Solutions(solutions) = store.query(q3)? {
        println!("{:<45} | {:<35} | {:<8}", "Call Node", "Source File", "Line");
        println!("{:-<45}-+-{:-<35}-+-{:-<8}", "", "", "");
        for sol in solutions {
            let sol = sol?;
            let call = sol.get("call").map(|t| t.to_string()).unwrap_or_default();
            let src = sol.get("src").map(|t| t.to_string()).unwrap_or_default();
            let line = sol.get("line").map(|t| t.to_string()).unwrap_or_default();
            let short_call = call.rsplit('#').last().unwrap_or(&call);
            println!("{:<45} | {:<35} | {:<8}", short_call.trim_matches('>'), src.trim_matches('"'), line.trim_matches('"'));
        }
    }

    println!("\n=== All SPARQL Queries Executed Successfully in Oxigraph ===");
    Ok(())
}
