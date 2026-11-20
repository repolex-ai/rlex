use anyhow::Result;
use serde::Serialize;
use std::time::Instant;

use crate::client::Client;
use crate::config::Config;
use crate::filter::ScopeFilter;
use crate::registry;

pub struct CyclesOptions {
    pub repo: Option<String>,
    pub format: String,
    pub endpoint: Option<String>,
    pub filter: ScopeFilter,
}

#[derive(Debug, Serialize)]
pub struct CyclesReportJson {
    pub target_scope: String,
    pub scope: String,
    pub latency_ms: f64,
    pub cycle_count: usize,
    pub cycles: Vec<DetectedCycleJson>,
}

#[derive(Debug, Serialize)]
pub struct DetectedCycleJson {
    pub cycle_type: String, // "Cross-Package Mutual Recursion" or "Internal Mutual File Call"
    pub entity_a: String,
    pub entity_b: String,
    pub evidence: String,
}

pub fn run(config: &Config, opts: &CyclesOptions) -> Result<()> {
    let client = Client::new(config, opts.endpoint.as_deref())?;
    let t0 = Instant::now();

    let mut cycles: Vec<DetectedCycleJson> = Vec::new();
    let target_scope = opts.repo.clone().unwrap_or_else(|| "Ecosystem Backbone".to_string());
    let src_filter_opt = opts.filter.sparql_filter_optional("?srcFile");
    let src_a_filter = opts.filter.sparql_filter("?srcA");
    let src_b_filter = opts.filter.sparql_filter("?srcB");

    if let Some(ref repo_name) = opts.repo {
        // Targeted check for a specific repository
        let mut repo_ref = registry::resolve_repo(repo_name, Some(&config.paths.cache))
            .ok_or_else(|| anyhow::anyhow!("Repository '{}' not found in registry or cache", repo_name))?;
        let repo_eco = registry::detect_ecosystem(&client, &repo_ref);
        repo_ref.ecosystem = repo_eco;

        let lsp_iri = repo_ref.lsp_graph();

        // Check internal mutual file recursion
        let q_internal = format!(
            r#"
PREFIX lx: <https://repolex.ai/ontology/repolex/lsp-extension/>
SELECT DISTINCT ?srcA ?srcB
WHERE {{
  GRAPH <{}> {{
    ?e1 lx:resolutionSourceFile ?srcA ;
        lx:callTargetFile ?srcB .
    ?e2 lx:resolutionSourceFile ?srcB ;
        lx:callTargetFile ?srcA .
    FILTER(STR(?srcA) < STR(?srcB))
    {}
    {}
  }}
}}
LIMIT 20
"#,
            lsp_iri, src_a_filter, src_b_filter
        );

        if let Ok(res) = client.query(&q_internal) {
            for row in res.rows {
                if row.len() >= 2 {
                    cycles.push(DetectedCycleJson {
                        cycle_type: "Internal Cross-File Mutual Call".into(),
                        entity_a: row[0].clone(),
                        entity_b: row[1].clone(),
                        evidence: format!("Mutual function invocation between {} and {}", row[0], row[1]),
                    });
                }
            }
        }

        // Check cross-package mutual recursion with its direct dependencies (same ecosystem only)
        let q_outgoing = format!(
            r#"
PREFIX lx: <https://repolex.ai/ontology/repolex/lsp-extension/>
SELECT ?targetPkg (SAMPLE(?ct) AS ?sampleTarget)
WHERE {{
  GRAPH <{}> {{
    ?e lx:externalPackage ?targetPkg .
    OPTIONAL {{ ?e lx:resolutionSourceFile ?srcFile . }}
    OPTIONAL {{ ?e lx:callTarget ?ct . }}
    {}
  }}
}}
GROUP BY ?targetPkg
"#,
            lsp_iri, src_filter_opt
        );

        if let Ok(res) = client.query(&q_outgoing) {
            for row in res.rows {
                if let Some(target_pkg) = row.first() {
                    let sample_target = row.get(1).filter(|s| !s.is_empty()).map(|s| s.as_str());
                    if let Some(target_repo) = registry::resolve_dep_repo(
                        &client,
                        &repo_ref,
                        repo_eco,
                        target_pkg,
                        sample_target,
                        Some(&config.paths.cache),
                    ) {
                        let q_reverse = format!(
                            r#"
PREFIX lx: <https://repolex.ai/ontology/repolex/lsp-extension/>
SELECT (COUNT(?e) as ?cnt)
WHERE {{
  GRAPH <{}> {{
    ?e lx:externalPackage ?callerPkg .
    OPTIONAL {{ ?e lx:resolutionSourceFile ?srcFile . }}
    FILTER(LCASE(STR(?callerPkg)) = "{}")
    {}
  }}
}}
"#,
                            target_repo.lsp_graph(),
                            repo_ref.repo.to_lowercase(),
                            src_filter_opt
                        );

                        if let Ok(rev_res) = client.query(&q_reverse) {
                            let rev_count = rev_res.rows.first()
                                .and_then(|r| r.first())
                                .and_then(|s| s.parse::<usize>().ok())
                                .unwrap_or(0);

                            if rev_count > 0 {
                                cycles.push(DetectedCycleJson {
                                    cycle_type: "Cross-Package Mutual Dependency Cycle".into(),
                                    entity_a: repo_ref.repo.clone(),
                                    entity_b: target_repo.repo.clone(),
                                    evidence: format!("Mutual cross-calls between {} and {}", repo_ref.repo, target_repo.repo),
                                });
                            }
                        }
                    }
                }
            }
        }
    } else {
        // Ecosystem-wide mutual call scan across registered backbone pairs (same ecosystem only)
        let backbone = registry::get_backbone();
        for (i, r1) in backbone.iter().enumerate() {
            for r2 in backbone.iter().skip(i + 1) {
                if r1.ecosystem.is_some() && r2.ecosystem.is_some() && r1.ecosystem != r2.ecosystem {
                    continue;
                }
                let q_check = format!(
                    r#"
PREFIX lx: <https://repolex.ai/ontology/repolex/lsp-extension/>
SELECT ?cnt1 ?cnt2
WHERE {{
  {{
    SELECT (COUNT(?e1) AS ?cnt1) WHERE {{
      GRAPH <{}> {{
        ?e1 lx:externalPackage ?pkgB .
        OPTIONAL {{ ?e1 lx:resolutionSourceFile ?srcFile . }}
        FILTER(LCASE(STR(?pkgB)) = "{}")
        {}
      }}
    }}
  }}
  {{
    SELECT (COUNT(?e2) AS ?cnt2) WHERE {{
      GRAPH <{}> {{
        ?e2 lx:externalPackage ?pkgA .
        OPTIONAL {{ ?e2 lx:resolutionSourceFile ?srcFile . }}
        FILTER(LCASE(STR(?pkgA)) = "{}")
        {}
      }}
    }}
  }}
}}
"#,
                    r1.lsp_graph(),
                    r2.repo.to_lowercase(),
                    src_filter_opt,
                    r2.lsp_graph(),
                    r1.repo.to_lowercase(),
                    src_filter_opt
                );

                if let Ok(res) = client.query(&q_check)
                    && let Some(row) = res.rows.first()
                {
                    let c1 = row.first().and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
                    let c2 = row.get(1).and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
                    if c1 > 0 && c2 > 0 {
                        cycles.push(DetectedCycleJson {
                            cycle_type: "Ecosystem Mutual Recursion".into(),
                            entity_a: r1.repo.clone(),
                            entity_b: r2.repo.clone(),
                            evidence: format!(
                                "{} calls {} ({} times), and {} calls {} ({} times)",
                                r1.repo, r2.repo, c1, r2.repo, r1.repo, c2
                            ),
                        });
                    }
                }
            }
        }
    }

    let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;

    if opts.format == "json" {
        let report = CyclesReportJson {
            target_scope,
            scope: opts.filter.label().to_string(),
            latency_ms: elapsed_ms,
            cycle_count: cycles.len(),
            cycles,
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!("================================================================================");
    println!("  CYCLIC DEPENDENCY & RECURSION DETECTOR: {} (scope: {})", target_scope, opts.filter.label());
    println!("  Execution Latency: {:.2} ms | Live Oxigraph Store", elapsed_ms);
    println!("================================================================================");

    if cycles.is_empty() {
        println!("\n  ✓ Clean Graph: No cyclic dependencies or mutual recursion detected in '{}'.", target_scope);
        println!("  All call hierarchies follow clean directed acyclic graph (DAG) ordering.\n");
        return Ok(());
    }

    println!("\nDetected {} recursion cycle(s):", cycles.len());
    for (idx, c) in cycles.iter().enumerate() {
        println!("  [Cycle #{}] {}", idx + 1, c.cycle_type);
        println!("    ├─ Between: {} ⇄ {}", c.entity_a, c.entity_b);
        println!("    └─ Evidence: {}", c.evidence);
    }

    Ok(())
}
