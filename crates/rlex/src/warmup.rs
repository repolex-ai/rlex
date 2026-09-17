use anyhow::Result;
use serde::Serialize;
use std::time::Instant;

use crate::client::Client;
use crate::config::Config;
use crate::registry;

#[derive(Debug, Serialize)]
pub struct WarmupReportJson {
    pub timestamp: String,
    pub total_graphs_warmed: usize,
    pub total_duration_ms: f64,
    pub warmed_graphs: Vec<WarmedGraphJson>,
    pub benchmark_probe_ms: f64,
}

#[derive(Debug, Serialize)]
pub struct WarmedGraphJson {
    pub repo: String,
    pub graph_type: String,
    pub graph_iri: String,
    pub quads_touched: usize,
    pub latency_ms: f64,
}

pub fn run(config: &Config, endpoint: Option<&str>, json: bool) -> Result<()> {
    let client = Client::new(config, endpoint)?;
    let t_start = Instant::now();

    let repos = registry::get_backbone();
    let mut warmed_graphs = Vec::new();

    if !json {
        println!("================================================================================");
        println!("          REPOLEX ROCKSDB SST INDEX & CACHE PRE-WARMING                         ");
        println!("================================================================================");
        println!("  Targeting {} ecosystem backbone repository graphs...", repos.len());
        println!("  Mode: {}", if client.is_remote() { "Remote HTTP (:7878)" } else { "Local RocksDB Read-Only" });
        println!("--------------------------------------------------------------------------------");
    }

    for r in &repos {
        let lsp_iri = r.lsp_graph();
        let q_lsp = format!(
            "SELECT (COUNT(?s) AS ?cnt) WHERE {{ GRAPH <{}> {{ ?s ?p ?o }} }}",
            lsp_iri
        );

        let t0 = Instant::now();
        let cnt = match client.query(&q_lsp) {
            Ok(res) => res.rows.first()
                .and_then(|row| row.first())
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0),
            Err(_) => 0,
        };
        let lat = t0.elapsed().as_secs_f64() * 1000.0;

        if cnt > 0 {
            if !json {
                println!(
                    "  [WARM] {:<25} (LSP)   {:>6} quads  in {:>5.2} ms",
                    format!("{}/{}", r.org, r.repo),
                    cnt,
                    lat
                );
            }
            warmed_graphs.push(WarmedGraphJson {
                repo: format!("{}/{}", r.org, r.repo),
                graph_type: "lsp".into(),
                graph_iri: lsp_iri,
                quads_touched: cnt,
                latency_ms: lat,
            });
        }
    }

    // Run verification probe across warmed cache: Web-to-Search 4-hop traversal
    let probe_q = r#"
PREFIX lx: <https://repolex.ai/ontology/repolex/lsp-extension/>
SELECT ?rlexFile ?actixFile ?regexFile ?ahoFile ?memchrTarget
WHERE {
  GRAPH <https://repolex.ai/r/repolex-ai/rlex/lsp/550a8e5a1a7b121bd970eff3e7575acd158f6bb8> {
    ?e1 lx:resolutionSourceFile ?rlexFile ;
        lx:externalPackage "actix-web" .
  }
  GRAPH <https://repolex.ai/r/actix/actix-web/lsp/5723cf486522d47aad26390cf5b02e95654ae225> {
    ?e2 lx:resolutionSourceFile ?actixFile ;
        lx:externalPackage "regex" .
  }
  GRAPH <https://repolex.ai/r/rust-lang/regex/lsp/25a15e272b3ae5aee76b525902c2ab91b0d9e12e> {
    ?e3 lx:resolutionSourceFile ?regexFile ;
        lx:externalPackage "aho-corasick" .
  }
  GRAPH <https://repolex.ai/r/BurntSushi/aho-corasick/lsp/d84a5073d5108fce1774b375105dfdb13fe4e81c> {
    ?e4 lx:resolutionSourceFile ?ahoFile ;
        lx:externalPackage "memchr" ;
        lx:callTarget ?memchrTarget .
  }
}
LIMIT 10
"#;

    let t_probe = Instant::now();
    let _ = client.query(probe_q);
    let probe_ms = t_probe.elapsed().as_secs_f64() * 1000.0;
    let total_ms = t_start.elapsed().as_secs_f64() * 1000.0;

    if json {
        let now = std::time::SystemTime::now();
        let since = now.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        let ts = format!("{}.{}", since.as_secs(), since.subsec_millis());
        let rep = WarmupReportJson {
            timestamp: ts,
            total_graphs_warmed: warmed_graphs.len(),
            total_duration_ms: total_ms,
            warmed_graphs,
            benchmark_probe_ms: probe_ms,
        };
        println!("{}", serde_json::to_string_pretty(&rep)?);
        return Ok(());
    }

    println!("--------------------------------------------------------------------------------");
    println!("  Cache Pre-Warming Completed:");
    println!("    • Backbone Graphs Pinned: {}", warmed_graphs.len());
    println!("    • Total Warmup Latency:   {:.2} ms", total_ms);
    println!("    • Post-Warm Traversal:    {:.2} ms (Web-to-Search 4-Hop)", probe_ms);
    println!("================================================================================");

    Ok(())
}
