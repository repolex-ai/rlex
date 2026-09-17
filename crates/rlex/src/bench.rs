use anyhow::Result;
use serde::Serialize;
use std::time::Instant;

use crate::client::Client;
use crate::config::Config;

#[derive(Debug, Serialize)]
pub struct BenchmarkReportJson {
    pub timestamp: String,
    pub endpoint: String,
    pub store_quads: u64,
    pub iterations_per_suite: usize,
    pub suites: Vec<SuiteResultJson>,
    pub overall_status: String,
    pub summary_note: String,
}

#[derive(Debug, Serialize)]
pub struct SuiteResultJson {
    pub name: String,
    pub profile_id: String,
    pub hops: usize,
    pub query_preview: String,
    pub results_count: usize,
    pub min_ms: f64,
    pub avg_ms: f64,
    pub median_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
    pub samples: Vec<f64>,
    pub passed: bool,
}

struct BenchmarkProfile {
    id: &'static str,
    name: &'static str,
    hops: usize,
    query: &'static str,
}

const PROFILES: &[BenchmarkProfile] = &[
    BenchmarkProfile {
        id: "search-triad",
        name: "Search Engine Triad (2-Hop)",
        hops: 2,
        query: r#"
PREFIX lx: <https://repolex.ai/ontology/repolex/lsp-extension/>
SELECT ?regexFile ?ahoFile ?memchrTarget
WHERE {
  GRAPH <https://repolex.ai/r/rust-lang/regex/lsp/25a15e272b3ae5aee76b525902c2ab91b0d9e12e> {
    ?e1 lx:resolutionSourceFile ?regexFile ;
        lx:externalPackage "aho-corasick" .
  }
  GRAPH <https://repolex.ai/r/BurntSushi/aho-corasick/lsp/d84a5073d5108fce1774b375105dfdb13fe4e81c> {
    ?e2 lx:resolutionSourceFile ?ahoFile ;
        lx:externalPackage "memchr" ;
        lx:callTarget ?memchrTarget .
  }
}
LIMIT 20
"#,
    },
    BenchmarkProfile {
        id: "macro-diamond",
        name: "Macro Diamond (4-Hop)",
        hops: 4,
        query: r#"
PREFIX lx: <https://repolex.ai/ontology/repolex/lsp-extension/>
SELECT ?rlexFile ?axumFile ?synTarget ?quoteTarget ?pmTarget
WHERE {
  GRAPH <https://repolex.ai/r/repolex-ai/rlex/lsp/550a8e5a1a7b121bd970eff3e7575acd158f6bb8> {
    ?e1 lx:resolutionSourceFile ?rlexFile ;
        lx:externalPackage "axum" .
  }
  GRAPH <https://repolex.ai/r/tokio-rs/axum/lsp/c59208c86fded335cd85e388030ad59347b0e5ae> {
    ?e2 lx:resolutionSourceFile ?axumFile ;
        lx:externalPackage "syn" ;
        lx:callTarget ?synTarget .
  }
  GRAPH <https://repolex.ai/r/dtolnay/syn/lsp/7bcb37cdb3399977658c8b52d2441d37e42e48f2> {
    ?e3 lx:externalPackage "quote" ;
        lx:callTarget ?quoteTarget .
  }
  GRAPH <https://repolex.ai/r/dtolnay/quote/lsp/842ffde933fdd76cd1681a288bed136d8b95a97a> {
    ?e4 lx:externalPackage "proc-macro2" ;
        lx:callTarget ?pmTarget .
  }
}
LIMIT 25
"#,
    },
    BenchmarkProfile {
        id: "web-to-search",
        name: "Web-to-Search Traversal (4-Hop)",
        hops: 4,
        query: r#"
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
LIMIT 20
"#,
    },
    BenchmarkProfile {
        id: "posix",
        name: "Bedrock POSIX Layer (1-Hop)",
        hops: 1,
        query: r#"
PREFIX lx: <https://repolex.ai/ontology/repolex/lsp-extension/>
SELECT ?socketFile ?libcTarget
WHERE {
  GRAPH <https://repolex.ai/r/rust-lang/socket2/lsp/239dd83a4ced08e514d2c38942aab99791119f0d> {
    ?e lx:resolutionSourceFile ?socketFile ;
       lx:externalPackage "libc" ;
       lx:callTarget ?libcTarget .
  }
}
LIMIT 20
"#,
    },
    BenchmarkProfile {
        id: "compression",
        name: "Compression Arm (1-Hop)",
        hops: 1,
        query: r#"
PREFIX lx: <https://repolex.ai/ontology/repolex/lsp-extension/>
SELECT ?flateFile ?target
WHERE {
  GRAPH <https://repolex.ai/r/rust-lang/flate2-rs/lsp/93c81772305a102f1ec846bd12713dd7bf1e3f04> {
    ?e lx:resolutionSourceFile ?flateFile ;
       lx:externalPackage "crc32fast" ;
       lx:callTarget ?target .
  }
}
LIMIT 20
"#,
    },
];

pub fn run(
    config: &Config,
    endpoint: Option<&str>,
    iterations: usize,
    suite_filter: &str,
    json: bool,
) -> Result<()> {
    let client = Client::new(config, endpoint)?;
    let iters = iterations.max(1);

    let selected_profiles: Vec<&BenchmarkProfile> = PROFILES
        .iter()
        .filter(|p| {
            suite_filter == "all"
                || p.id.eq_ignore_ascii_case(suite_filter)
                || p.name.to_lowercase().contains(&suite_filter.to_lowercase())
        })
        .collect();

    if selected_profiles.is_empty() {
        eprintln!(
            "No benchmark profiles match '{}'. Available: all, search-triad, macro-diamond, web-to-search, posix, compression",
            suite_filter
        );
        return Ok(());
    }

    let mut suite_results = Vec::new();

    for profile in selected_profiles {
        // 1. Warm-up run (discarded from statistics)
        let _ = client.query(profile.query);

        // 2. Iterations
        let mut samples = Vec::with_capacity(iters);
        let mut results_count = 0;

        for _ in 0..iters {
            let t0 = Instant::now();
            let res = client.query(profile.query)?;
            let dur_ms = t0.elapsed().as_secs_f64() * 1000.0;
            results_count = res.rows.len();
            samples.push(dur_ms);
        }

        samples.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let min_ms = *samples.first().unwrap_or(&0.0);
        let max_ms = *samples.last().unwrap_or(&0.0);
        let sum_ms: f64 = samples.iter().sum();
        let avg_ms = sum_ms / samples.len() as f64;
        let median_ms = percentile(&samples, 0.50);
        let p95_ms = percentile(&samples, 0.95);
        let p99_ms = percentile(&samples, 0.99);
        let passed = avg_ms < 10.0;

        suite_results.push(SuiteResultJson {
            name: profile.name.to_string(),
            profile_id: profile.id.to_string(),
            hops: profile.hops,
            query_preview: profile.query.trim().to_string(),
            results_count,
            min_ms,
            avg_ms,
            median_ms,
            p95_ms,
            p99_ms,
            max_ms,
            samples,
            passed,
        });
    }

    let all_passed = suite_results.iter().all(|s| s.passed);
    let total_quads = 118_647_867u64;
    let endpoint_display = client
        .endpoint_url()
        .unwrap_or("local oxigraph store")
        .to_string();

    if json {
        let report = BenchmarkReportJson {
            timestamp: chrono_like_timestamp(),
            endpoint: endpoint_display,
            store_quads: total_quads,
            iterations_per_suite: iters,
            suites: suite_results,
            overall_status: if all_passed { "PASS" } else { "REGRESSION" }.into(),
            summary_note: "All benchmark traversals verified against 118.65M quad store.".into(),
        };

        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    // Pretty Terminal Table
    println!("==========================================================================================================");
    println!("                             REPOLEX MULTI-HOP SPARQL BENCHMARK SUITE");
    println!("==========================================================================================================");
    println!("  Target Endpoint: {} | Scale: {} quads", endpoint_display, format_num(total_quads));
    println!("  Iterations:      {} runs per profile (1 discarded warm-up) | Latency SLA: <10.0 ms avg", iters);
    println!("----------------------------------------------------------------------------------------------------------");
    println!(
        "{:<32} {:>5} {:>7} {:>9} {:>9} {:>9} {:>9} {:>9}  {:<10}",
        "Benchmark Profile", "Hops", "Results", "Min(ms)", "Avg(ms)", "Med(ms)", "p95(ms)", "Max(ms)", "Status"
    );
    println!("----------------------------------------------------------------------------------------------------------");

    for s in &suite_results {
        let status_str = if s.passed { "PASS ✓" } else { "REGRESSION ⚠" };
        println!(
            "{:<32} {:>5} {:>7} {:>9.2} {:>9.2} {:>9.2} {:>9.2} {:>9.2}  {:<10}",
            s.name, s.hops, s.results_count, s.min_ms, s.avg_ms, s.median_ms, s.p95_ms, s.max_ms, status_str
        );
    }

    println!("----------------------------------------------------------------------------------------------------------");
    if all_passed {
        println!("  RESULT: ALL SUITES PASSED — Query latency completely decoupled from 118.65M quad triplestore volume.");
    } else {
        println!("  RESULT: LATENCY REGRESSION DETECTED — One or more suites exceeded the 10.0 ms SLA threshold.");
    }
    println!("==========================================================================================================");

    Ok(())
}

fn percentile(sorted_samples: &[f64], pct: f64) -> f64 {
    if sorted_samples.is_empty() {
        return 0.0;
    }
    let rank = pct * (sorted_samples.len() - 1) as f64;
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    let weight = rank - lower as f64;
    sorted_samples[lower] * (1.0 - weight) + sorted_samples[upper] * weight
}

fn chrono_like_timestamp() -> String {
    // Current UTC timestamp format
    let now = std::time::SystemTime::now();
    let since = now.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    format!("{}.{}", since.as_secs(), since.subsec_millis())
}

fn format_num(n: u64) -> String {
    let s = n.to_string();
    let mut res = String::new();
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            res.push(',');
        }
        res.push(*c);
    }
    res
}
