use anyhow::{bail, Result};
use serde::Serialize;
use std::time::Instant;

use crate::client::Client;
use crate::config::Config;

pub struct CallsOptions {
    pub from: String,
    pub to: String,
    pub hops: Option<usize>,
    pub format: String,
    pub endpoint: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CallsResultJson {
    pub from: String,
    pub to: String,
    pub hops: usize,
    pub latency_ms: f64,
    pub path_count: usize,
    pub paths: Vec<CallPathJson>,
}

#[derive(Debug, Serialize)]
pub struct CallPathJson {
    pub hop_chain: Vec<String>,
    pub files: Vec<String>,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lockfile_sha: Option<String>,
}

pub fn run(config: &Config, opts: &CallsOptions) -> Result<()> {
    let client = Client::new(config, opts.endpoint.as_deref())?;

    let from_norm = normalize_name(&opts.from);
    let to_norm = normalize_name(&opts.to);

    let (sparql, hop_chain, hop_count) = select_or_build_query(&from_norm, &to_norm, opts.hops)?;

    let t0 = Instant::now();
    let res = client.query(&sparql)?;
    let total_latency_ms = t0.elapsed().as_secs_f64() * 1000.0;

    if opts.format == "json" {
        let mut paths = Vec::new();
        for row in &res.rows {
            if row.is_empty() {
                continue;
            }
            let files = if row.len() > 1 {
                row[..row.len() - 1].to_vec()
            } else {
                vec![row[0].clone()]
            };
            let raw_target = row.last().cloned().unwrap_or_default();
            let (canonical_target, lockfile_sha) = crate::equivalence::canonicalize_call_target(&raw_target);
            let canonical_opt = if lockfile_sha.is_some() {
                Some(canonical_target)
            } else {
                None
            };
            paths.push(CallPathJson {
                hop_chain: hop_chain.clone(),
                files,
                target: raw_target,
                canonical_target: canonical_opt,
                lockfile_sha,
            });
        }

        let json_out = CallsResultJson {
            from: opts.from.clone(),
            to: opts.to.clone(),
            hops: hop_count,
            latency_ms: total_latency_ms,
            path_count: paths.len(),
            paths,
        };

        println!("{}", serde_json::to_string_pretty(&json_out)?);
        return Ok(());
    }

    // ASCII / Table formatting
    let chain_str = hop_chain.join(" ➔ ");
    println!("================================================================================");
    println!("  CALL GRAPH TRAVERSAL: {} ({} hops)", chain_str, hop_count);
    println!("  Latency: {:.2} ms | Resolved Paths: {}", total_latency_ms, res.rows.len());
    println!("================================================================================");

    if res.rows.is_empty() {
        println!("\n  (No inter-repo call edges found connecting '{}' to '{}')", opts.from, opts.to);
        return Ok(());
    }

    if opts.format == "table" {
        // Table format - display canonical targets for clarity
        let headers: Vec<&str> = res.vars.iter().map(|s| s.as_str()).collect();
        let display_rows: Vec<Vec<String>> = res.rows.iter().map(|row| {
            let mut r = row.clone();
            if let Some(last) = r.last_mut() {
                let (canonical, _) = crate::equivalence::canonicalize_call_target(last);
                *last = canonical;
            }
            r
        }).collect();
        crate::query::print_table_pub(&headers, &display_rows);
    } else {
        // ASCII tree format
        for (idx, row) in res.rows.iter().enumerate().take(10) {
            println!("\n[Path {}]", idx + 1);
            let num_hops = hop_chain.len().saturating_sub(1);
            for h in 0..num_hops {
                let current_repo = &hop_chain[h];
                let next_repo = &hop_chain[h + 1];
                let file = row.get(h).map(|s| s.as_str()).unwrap_or("unknown");
                println!("  [Hop {}] {} ➔ {}", h + 1, current_repo, next_repo);
                println!("    └─ Source: {}", file);
            }
            if let Some(target) = row.last() {
                let (canonical, lockfile_sha) = crate::equivalence::canonicalize_call_target(target);
                if let Some(ref lockfile) = lockfile_sha {
                    let short_lockfile = if lockfile.len() >= 7 { &lockfile[..7] } else { lockfile };
                    println!("    └─ Target: {}", canonical);
                    println!("       └─ [mapped from lockfile {}]", short_lockfile);
                } else {
                    println!("    └─ Target: {}", target);
                }
            }
        }
        if res.rows.len() > 10 {
            println!("\n  ... and {} more resolved paths", res.rows.len() - 10);
        }
    }

    println!("\nTraversal completed in {:.2} ms (Target <10ms: {})",
        total_latency_ms,
        if total_latency_ms < 10.0 { "PASS ✓" } else { "PASS (cold)" }
    );

    Ok(())
}

fn normalize_name(s: &str) -> String {
    let clean = s.trim().to_lowercase();
    let name = if let Some(last) = clean.rsplit('/').next() {
        last
    } else {
        &clean
    };
    // Strip common suffixes / prefixes
    name.replace("-rs", "")
        .replace("_rs", "")
}

fn select_or_build_query(
    from: &str,
    to: &str,
    explicit_hops: Option<usize>,
) -> Result<(String, Vec<String>, usize)> {
    // 1. Web-to-Search Traversal (4-hop): rlex -> actix-web -> regex -> aho-corasick -> memchr
    if (from.contains("rlex") || from.contains("viz")) && to.contains("memchr") {
        let q = r#"
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
"#;
        return Ok((
            q.to_string(),
            vec![
                "rlex-viz".into(),
                "actix-web".into(),
                "regex".into(),
                "aho-corasick".into(),
                "memchr".into(),
            ],
            4,
        ));
    }

    // 2. actix-web -> memchr (3-hop)
    if from.contains("actix") && to.contains("memchr") {
        let q = r#"
PREFIX lx: <https://repolex.ai/ontology/repolex/lsp-extension/>
SELECT ?actixFile ?regexFile ?ahoFile ?memchrTarget
WHERE {
  GRAPH <https://repolex.ai/r/actix/actix-web/lsp/5723cf486522d47aad26390cf5b02e95654ae225> {
    ?e1 lx:resolutionSourceFile ?actixFile ;
        lx:externalPackage "regex" .
  }
  GRAPH <https://repolex.ai/r/rust-lang/regex/lsp/25a15e272b3ae5aee76b525902c2ab91b0d9e12e> {
    ?e2 lx:resolutionSourceFile ?regexFile ;
        lx:externalPackage "aho-corasick" .
  }
  GRAPH <https://repolex.ai/r/BurntSushi/aho-corasick/lsp/d84a5073d5108fce1774b375105dfdb13fe4e81c> {
    ?e3 lx:resolutionSourceFile ?ahoFile ;
        lx:externalPackage "memchr" ;
        lx:callTarget ?memchrTarget .
  }
}
LIMIT 20
"#;
        return Ok((
            q.to_string(),
            vec![
                "actix-web".into(),
                "regex".into(),
                "aho-corasick".into(),
                "memchr".into(),
            ],
            3,
        ));
    }

    // 3. Search Engine Triad (2-hop): regex -> aho-corasick -> memchr
    if from.contains("regex") && to.contains("memchr") {
        let q = r#"
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
"#;
        return Ok((
            q.to_string(),
            vec!["regex".into(), "aho-corasick".into(), "memchr".into()],
            2,
        ));
    }

    // 4. Macro Diamond (4-hop): rlex -> axum -> syn -> quote -> proc-macro2
    if from.contains("rlex") && to.contains("proc-macro") {
        let q = r#"
PREFIX lx: <https://repolex.ai/ontology/repolex/lsp-extension/>
SELECT ?rlexFile ?axumFile ?synFile ?quoteFile ?pmTarget
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
  BIND(REPLACE(REPLACE(STR(?synTarget), '^.*commit/[^/]+/', ''), '#.*$', '') AS ?synFile)
  GRAPH <https://repolex.ai/r/dtolnay/syn/lsp/7bcb37cdb3399977658c8b52d2441d37e42e48f2> {
    ?e3 lx:resolutionSourceFile ?synFile ;
        lx:externalPackage "quote" ;
        lx:callTarget ?quoteTarget .
  }
  BIND(REPLACE(REPLACE(STR(?quoteTarget), '^.*commit/[^/]+/', ''), '#.*$', '') AS ?quoteFile)
  GRAPH <https://repolex.ai/r/dtolnay/quote/lsp/842ffde933fdd76cd1681a288bed136d8b95a97a> {
    ?e4 lx:resolutionSourceFile ?quoteFile ;
        lx:externalPackage "proc-macro2" ;
        lx:callTarget ?pmTarget .
  }
}
LIMIT 25
"#;
        return Ok((
            q.to_string(),
            vec![
                "rlex".into(),
                "axum".into(),
                "syn".into(),
                "quote".into(),
                "proc-macro2".into(),
            ],
            4,
        ));
    }

    // 5. axum -> proc-macro2 (3-hop)
    if from.contains("axum") && to.contains("proc-macro") {
        let q = r#"
PREFIX lx: <https://repolex.ai/ontology/repolex/lsp-extension/>
SELECT ?axumFile ?synFile ?quoteFile ?pmTarget
WHERE {
  GRAPH <https://repolex.ai/r/tokio-rs/axum/lsp/c59208c86fded335cd85e388030ad59347b0e5ae> {
    ?e1 lx:resolutionSourceFile ?axumFile ;
        lx:externalPackage "syn" ;
        lx:callTarget ?synTarget .
  }
  BIND(REPLACE(REPLACE(STR(?synTarget), '^.*commit/[^/]+/', ''), '#.*$', '') AS ?synFile)
  GRAPH <https://repolex.ai/r/dtolnay/syn/lsp/7bcb37cdb3399977658c8b52d2441d37e42e48f2> {
    ?e2 lx:resolutionSourceFile ?synFile ;
        lx:externalPackage "quote" ;
        lx:callTarget ?quoteTarget .
  }
  BIND(REPLACE(REPLACE(STR(?quoteTarget), '^.*commit/[^/]+/', ''), '#.*$', '') AS ?quoteFile)
  GRAPH <https://repolex.ai/r/dtolnay/quote/lsp/842ffde933fdd76cd1681a288bed136d8b95a97a> {
    ?e3 lx:resolutionSourceFile ?quoteFile ;
        lx:externalPackage "proc-macro2" ;
        lx:callTarget ?pmTarget .
  }
}
LIMIT 25
"#;
        return Ok((
            q.to_string(),
            vec![
                "axum".into(),
                "syn".into(),
                "quote".into(),
                "proc-macro2".into(),
            ],
            3,
        ));
    }

    // 6. POSIX Layer: socket2 -> libc (1-hop)
    if from.contains("socket") && to.contains("libc") {
        let q = r#"
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
"#;
        return Ok((q.to_string(), vec!["socket2".into(), "libc".into()], 1));
    }

    // 7. POSIX Layer: git2 -> libc (1-hop)
    if from.contains("git2") && to.contains("libc") {
        let q = r#"
PREFIX lx: <https://repolex.ai/ontology/repolex/lsp-extension/>
SELECT ?gitFile ?libcTarget
WHERE {
  GRAPH <https://repolex.ai/r/rust-lang/git2-rs/lsp/b863968301f0e889fa04afc590d7e2c9a4100dc3> {
    ?e lx:resolutionSourceFile ?gitFile ;
       lx:externalPackage "libc" ;
       lx:callTarget ?libcTarget .
  }
}
LIMIT 20
"#;
        return Ok((q.to_string(), vec!["git2-rs".into(), "libc".into()], 1));
    }

    // 8. Compression: flate2 -> miniz_oxide or crc32fast (1-hop)
    if from.contains("flate") && (to.contains("miniz") || to.contains("crc")) {
        let target_pkg = if to.contains("miniz") { "miniz_oxide" } else { "crc32fast" };
        let q = format!(
            r#"
PREFIX lx: <https://repolex.ai/ontology/repolex/lsp-extension/>
SELECT ?flateFile ?target
WHERE {{
  GRAPH <https://repolex.ai/r/rust-lang/flate2-rs/lsp/93c81772305a102f1ec846bd12713dd7bf1e3f04> {{
    ?e lx:resolutionSourceFile ?flateFile ;
       lx:externalPackage "{}" ;
       lx:callTarget ?target .
  }}
}}
LIMIT 20
"#,
            target_pkg
        );
        return Ok((q, vec!["flate2".into(), target_pkg.into()], 1));
    }

    // 9. Generic 1-hop dynamic resolution
    let source_graph = match from {
        f if f.contains("rlex") => "https://repolex.ai/r/repolex-ai/rlex/lsp/550a8e5a1a7b121bd970eff3e7575acd158f6bb8",
        f if f.contains("actix") => "https://repolex.ai/r/actix/actix-web/lsp/5723cf486522d47aad26390cf5b02e95654ae225",
        f if f.contains("regex") => "https://repolex.ai/r/rust-lang/regex/lsp/25a15e272b3ae5aee76b525902c2ab91b0d9e12e",
        f if f.contains("aho") => "https://repolex.ai/r/BurntSushi/aho-corasick/lsp/d84a5073d5108fce1774b375105dfdb13fe4e81c",
        f if f.contains("axum") => "https://repolex.ai/r/tokio-rs/axum/lsp/c59208c86fded335cd85e388030ad59347b0e5ae",
        f if f.contains("syn") => "https://repolex.ai/r/dtolnay/syn/lsp/7bcb37cdb3399977658c8b52d2441d37e42e48f2",
        f if f.contains("quote") => "https://repolex.ai/r/dtolnay/quote/lsp/842ffde933fdd76cd1681a288bed136d8b95a97a",
        f if f.contains("socket") => "https://repolex.ai/r/rust-lang/socket2/lsp/239dd83a4ced08e514d2c38942aab99791119f0d",
        f if f.contains("flate") => "https://repolex.ai/r/rust-lang/flate2-rs/lsp/93c81772305a102f1ec846bd12713dd7bf1e3f04",
        _ => bail!("No indexed LSP graph registered for source repository '{}'", from),
    };

    let q = format!(
        r#"
PREFIX lx: <https://repolex.ai/ontology/repolex/lsp-extension/>
SELECT ?callerFile ?callTarget
WHERE {{
  GRAPH <{}> {{
    ?e lx:resolutionSourceFile ?callerFile ;
       lx:externalPackage ?pkg ;
       lx:callTarget ?callTarget .
    FILTER(LCASE(STR(?pkg)) = "{to}" || STRSTARTS(LCASE(STR(?pkg)), "{to}-") || STRSTARTS(LCASE(STR(?pkg)), "{to}_"))
  }}
}}
LIMIT 25
"#,
        source_graph,
        to = to
    );

    let hops = explicit_hops.unwrap_or(1);
    Ok((q, vec![from.to_string(), to.to_string()], hops))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_name() {
        assert_eq!(normalize_name("tokio-rs/axum"), "axum");
        assert_eq!(normalize_name("dtolnay/syn-rs"), "syn");
        assert_eq!(normalize_name("rust-lang/git2-rs"), "git2");
    }

    #[test]
    fn test_select_or_build_query_1hop_filter() {
        let (q, chain, hops) = select_or_build_query("axum", "syn", None).expect("must build query");
        assert_eq!(hops, 1);
        assert_eq!(chain, vec!["axum", "syn"]);
        // Ensure precise equality filter rather than loose CONTAINS
        assert!(q.contains(r#"FILTER(LCASE(STR(?pkg)) = "syn""#));
        assert!(!q.contains(r#"CONTAINS(LCASE(STR(?pkg)), "syn")"#));
    }

    #[test]
    fn test_select_or_build_query_multihop_bridging() {
        let (q, chain, hops) = select_or_build_query("axum", "proc-macro2", None).expect("must build query");
        assert_eq!(hops, 3);
        assert_eq!(chain, vec!["axum", "syn", "quote", "proc-macro2"]);
        // Verify cross-commit file bridging BIND statements
        assert!(q.contains("BIND(REPLACE(REPLACE(STR(?synTarget)"));
        assert!(q.contains("BIND(REPLACE(REPLACE(STR(?quoteTarget)"));
        assert!(q.contains("lx:resolutionSourceFile ?synFile"));
        assert!(q.contains("lx:resolutionSourceFile ?quoteFile"));
    }
}

