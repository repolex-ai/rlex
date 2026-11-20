use anyhow::Result;
use serde::Serialize;
use std::collections::HashMap;
use std::time::Instant;

use crate::client::Client;
use crate::config::Config;
use crate::filter::ScopeFilter;
use crate::registry;

pub struct DiamondOptions {
    pub from: String,
    pub to: Option<String>,
    pub format: String,
    pub endpoint: Option<String>,
    pub filter: ScopeFilter,
}

#[derive(Debug, Serialize)]
pub struct DiamondReportJson {
    pub root: String,
    pub scope: String,
    pub latency_ms: f64,
    pub diamond_count: usize,
    pub diamonds: Vec<DiamondConvergenceJson>,
}

#[derive(Debug, Serialize)]
pub struct DiamondConvergenceJson {
    pub target: String,
    pub convergence_type: String, // "Direct + Transitive" or "Dual Transitive (B and C)"
    pub paths: Vec<Vec<String>>,
    pub total_calls_to_target: usize,
}

pub fn run(config: &Config, opts: &DiamondOptions) -> Result<()> {
    let client = Client::new(config, opts.endpoint.as_deref())?;
    let t0 = Instant::now();

    let mut root = registry::resolve_repo(&opts.from, Some(&config.paths.cache))
        .ok_or_else(|| anyhow::anyhow!("Repository '{}' not found in registry or cache", opts.from))?;
    let root_eco = registry::detect_ecosystem(&client, &root);
    root.ecosystem = root_eco;

    let root_name = root.repo.clone();
    let src_filter = opts.filter.sparql_filter_optional("?srcFile");

    // 1. Get direct outgoing dependencies from root
    let root_lsp = root.lsp_graph();
    let q_direct = format!(
        r#"
PREFIX lx: <https://repolex.ai/ontology/repolex/lsp-extension/>
SELECT ?targetPkg (COUNT(?e) as ?cnt) (SAMPLE(?ct) as ?sampleTarget)
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
        root_lsp, src_filter
    );

    let res_direct = client.query(&q_direct)?;
    let mut direct_calls: HashMap<String, usize> = HashMap::new();
    let mut direct_sample_targets: HashMap<String, String> = HashMap::new();
    for row in res_direct.rows {
        if let Some(pkg) = row.first() {
            let count = row.get(1).and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
            if count > 0 {
                direct_calls.insert(pkg.clone(), count);
                if let Some(ct) = row.get(2)
                    && !ct.is_empty()
                {
                    direct_sample_targets.insert(pkg.clone(), ct.clone());
                }
            }
        }
    }

    // 2. For each direct dependency resolved strictly within ecosystem, get its outgoing packages
    let mut intermediate_calls: HashMap<String, HashMap<String, usize>> = HashMap::new();
    for direct_pkg in direct_calls.keys() {
        let sample_target = direct_sample_targets.get(direct_pkg).map(|s| s.as_str());
        if let Some(direct_repo) = registry::resolve_dep_repo(
            &client,
            &root,
            root_eco,
            direct_pkg,
            sample_target,
            Some(&config.paths.cache),
        ) {
            let lsp_iri = direct_repo.lsp_graph();
            let q_sub = format!(
                r#"
PREFIX lx: <https://repolex.ai/ontology/repolex/lsp-extension/>
SELECT ?targetPkg (COUNT(?e) as ?cnt)
WHERE {{
  GRAPH <{}> {{
    ?e lx:externalPackage ?targetPkg .
    OPTIONAL {{ ?e lx:resolutionSourceFile ?srcFile . }}
    {}
  }}
}}
GROUP BY ?targetPkg
"#,
                lsp_iri, src_filter
            );

            if let Ok(res_sub) = client.query(&q_sub) {
                let mut map = HashMap::new();
                for row in res_sub.rows {
                    if let Some(pkg) = row.first() {
                        let count = row.get(1).and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
                        if count > 0 {
                            map.insert(pkg.clone(), count);
                        }
                    }
                }
                intermediate_calls.insert(direct_pkg.clone(), map);
            }
        }
    }

    // 3. Find diamond convergences:
    // Case A: root -> target (direct) AND root -> B -> target (transitive)
    // Case B: root -> B -> target AND root -> C -> target (where B != C)
    let mut diamonds: Vec<DiamondConvergenceJson> = Vec::new();

    // Map all reachable targets -> list of paths
    let mut target_paths: HashMap<String, Vec<Vec<String>>> = HashMap::new();
    let mut target_call_counts: HashMap<String, usize> = HashMap::new();

    // Direct paths
    for (pkg, cnt) in &direct_calls {
        target_paths
            .entry(pkg.clone())
            .or_default()
            .push(vec![root_name.clone(), pkg.clone()]);
        *target_call_counts.entry(pkg.clone()).or_default() += *cnt;
    }

    // 2-hop paths
    for (b, sub_map) in &intermediate_calls {
        for (target, cnt) in sub_map {
            target_paths
                .entry(target.clone())
                .or_default()
                .push(vec![root_name.clone(), b.clone(), target.clone()]);
            *target_call_counts.entry(target.clone()).or_default() += *cnt;
        }
    }

    // Identify targets with 2 or more distinct paths
    for (target, paths) in target_paths {
        if target.to_lowercase() == root_name.to_lowercase() {
            continue;
        }
        if paths.len() >= 2 {
            let has_direct = paths.iter().any(|p| p.len() == 2);
            let conv_type = if has_direct {
                "Direct + Transitive Shortcut".to_string()
            } else {
                "Multi-Branch Transitive Convergence".to_string()
            };

            let total_calls = target_call_counts.get(&target).copied().unwrap_or(0);

            diamonds.push(DiamondConvergenceJson {
                target,
                convergence_type: conv_type,
                paths,
                total_calls_to_target: total_calls,
            });
        }
    }

    diamonds.sort_by(|a, b| b.total_calls_to_target.cmp(&a.total_calls_to_target));

    // Optional filter by target
    if let Some(ref to_filter) = opts.to {
        let tf = to_filter.to_lowercase();
        diamonds.retain(|d| d.target.to_lowercase().contains(&tf));
    }

    let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;

    if opts.format == "json" {
        let report = DiamondReportJson {
            root: root_name,
            scope: opts.filter.label().to_string(),
            latency_ms: elapsed_ms,
            diamond_count: diamonds.len(),
            diamonds,
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!("================================================================================");
    println!("  DIAMOND CONVERGENCE SOLVER: {} (scope: {}, {} converging targets found)",
        root_name, opts.filter.label(), diamonds.len());
    println!("  Traversed in {:.2} ms across live Oxigraph store", elapsed_ms);
    println!("================================================================================");

    if diamonds.is_empty() {
        println!("\n  (No diamond dependency convergence patterns found for '{}')", opts.from);
        return Ok(());
    }

    for (i, d) in diamonds.iter().enumerate() {
        println!("\n[Diamond #{}] Target: {} ({})", i + 1, d.target, d.convergence_type);
        println!("  Total Cross-Repo Calls: {}", d.total_calls_to_target);
        println!("  Converging Ingress Paths:");
        for p in &d.paths {
            let path_str = p.join(" ➔ ");
            println!("    └─ {}", path_str);
        }
    }

    let status = if elapsed_ms < 10.0 { "PASS ✓" } else { "FAIL" };
    println!("\nDiamond analysis completed in {:.2} ms (Target <10ms: {})", elapsed_ms, status);

    Ok(())
}
