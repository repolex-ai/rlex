use clap::{Parser, Subcommand};
use anyhow::Result;

mod audit;
mod bench;
mod calls;
mod catalog;
mod client;
mod closure;
mod compaction;
mod config;
mod cycles;
mod diamond;
mod download;
mod equivalence;
mod index;
mod load;
mod moreinfo;
mod query;
mod registry;
mod scan_guard;
mod serve;
mod viz;
mod warmup;

#[derive(Parser)]
#[command(name = "rlex", about = "SPARQL query tool for repolex knowledge graphs")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Sync the forx-index (clone or pull)
    Sync,

    /// Search and browse repos in the forx-index
    Repos {
        /// Search term or org/repo for detail view
        query: Option<String>,
    },

    /// Download graph data for a repo at a specific tag or commit
    Download {
        /// Repository in org/repo format
        repo: String,
        /// Tag name or commit sha
        target: String,
        /// Graph types to download (default: all)
        #[arg(short, long)]
        graphs: Option<Vec<String>>,
    },

    /// Show what's downloaded in the local cache
    Cache,

    /// Load cached graph files into the oxigraph store
    Load {
        /// Repository in org/repo format
        repo: String,
        /// Specific commit sha (prefix match). If omitted, loads all cached commits.
        commit: Option<String>,
    },

    /// Run a SPARQL query against the local store
    Query {
        /// SPARQL query string
        sparql: String,
        /// Output format: json, csv, tsv, table, turtle, ntriples, json-ld
        #[arg(short, long, default_value = "table")]
        format: String,
        /// Query only the bare default graph. By default rlex treats the
        /// default graph as the union of all named graphs, so a query without
        /// a GRAPH clause still finds forx's per-repo data instead of silently
        /// returning nothing. Pass this for strict SPARQL default-graph semantics.
        #[arg(long)]
        no_union: bool,
        /// Abort execution if unindexed full-store scan hazards are detected (e.g. the CONTAINS trap)
        #[arg(long)]
        strict_scan_guard: bool,
    },

    /// Start SPARQL HTTP endpoint + viz API + catalog API
    Serve {
        /// Port for HTTP server
        #[arg(short, long, default_value = "7878")]
        port: u16,
        /// Directory containing index.html for viz UI
        #[arg(long)]
        viz_dir: Option<String>,
        /// Stop a running server
        #[arg(long)]
        stop: bool,
        /// Run in foreground (don't background)
        #[arg(long)]
        foreground: bool,
        /// Don't open browser
        #[arg(long)]
        no_browser: bool,
    },

    /// Start viz UI + SPARQL endpoint in background (:3000 + :7878)
    Viz {
        /// Port for viz web UI
        #[arg(short, long, default_value = "3000")]
        port: u16,
        /// Port for SPARQL endpoint
        #[arg(long, default_value = "7878")]
        sparql_port: u16,
        /// Stop all background servers
        #[arg(long)]
        stop: bool,
    },

    /// Show current configuration
    Config,

    /// Trace multi-hop inter-repo call graph traversals
    Calls {
        /// Source repository (e.g. actix-web, rlex, regex, socket2, flate2)
        #[arg(short, long)]
        from: String,

        /// Target repository (e.g. memchr, proc-macro2, libc, miniz_oxide, crc32fast)
        #[arg(short, long)]
        to: String,

        /// Explicit hop limit or count
        #[arg(long)]
        hops: Option<usize>,

        /// Output format: ascii (default), table, json
        #[arg(long, default_value = "ascii")]
        format: String,

        /// SPARQL endpoint URL (default: local store with fallback to http://localhost:7878/query)
        #[arg(short, long)]
        endpoint: Option<String>,
    },

    /// Inspect Oxigraph store quad volume, repository inventory, or audit a specific repository
    Audit {
        /// Target repository to audit (e.g. pan, git-lex, repolex-ai/pan).
        /// If omitted, audits global triplestore health.
        #[arg(value_name = "REPO")]
        repo_pos: Option<String>,

        /// Target repository to audit (named option: --repo pan)
        #[arg(short, long, value_name = "REPO")]
        repo: Option<String>,

        /// Specific commit SHA (prefix or full SHA). Defaults to latest cached/loaded commit.
        #[arg(short, long)]
        commit: Option<String>,

        /// Output format: markdown (default for repo), ascii, json
        #[arg(short, long)]
        format: Option<String>,

        /// Emit structured JSON output
        #[arg(long)]
        json: bool,

        /// SPARQL endpoint URL (default: queries local store or background server)
        #[arg(short, long)]
        endpoint: Option<String>,
    },

    /// Run automated multi-hop SPARQL benchmark suite and regression monitor
    Bench {
        /// SPARQL endpoint URL (default: http://localhost:7878/query or local store)
        #[arg(short, long)]
        endpoint: Option<String>,

        /// Number of timed iterations per benchmark profile
        #[arg(short, long, default_value = "10")]
        iterations: usize,

        /// Benchmark suite: all, search-triad, macro-diamond, web-to-search, posix, compression
        #[arg(short, long, default_value = "all")]
        suite: String,

        /// Output results as JSON for CI/CD tracking
        #[arg(long)]
        json: bool,
    },

    /// Discover deep transitive call graph closures across repository boundaries
    Closure {
        /// Source repository (e.g. regex, actix-web, syn, rlex)
        #[arg(short, long)]
        from: String,

        /// Optional filter for target repository / package
        #[arg(short, long)]
        to: Option<String>,

        /// Maximum transitive hop depth (1-6, default: 3)
        #[arg(short, long, default_value = "3")]
        depth: usize,

        /// Output format: ascii (default), table, json
        #[arg(long, default_value = "ascii")]
        format: String,

        /// SPARQL endpoint URL (default: local store or http://localhost:7878/query)
        #[arg(short, long)]
        endpoint: Option<String>,
    },

    /// Solve diamond dependency convergence patterns across multiple branches
    Diamond {
        /// Root repository to analyze (e.g. syn, regex, axum)
        #[arg(short, long)]
        from: String,

        /// Optional filter for converged target repository / package
        #[arg(short, long)]
        to: Option<String>,

        /// Output format: ascii (default), json
        #[arg(long, default_value = "ascii")]
        format: String,

        /// SPARQL endpoint URL (default: local store or http://localhost:7878/query)
        #[arg(short, long)]
        endpoint: Option<String>,
    },

    /// Detect cyclic dependencies and mutual recursion patterns
    Cycles {
        /// Target repository to inspect (default: scans ecosystem backbone)
        #[arg(short, long)]
        repo: Option<String>,

        /// Output format: ascii (default), json
        #[arg(long, default_value = "ascii")]
        format: String,

        /// SPARQL endpoint URL (default: local store or http://localhost:7878/query)
        #[arg(short, long)]
        endpoint: Option<String>,
    },

    /// Pre-warm RocksDB SST index and bloom filter blocks for core ecosystem graphs
    Warmup {
        /// Emit structured JSON output
        #[arg(long)]
        json: bool,

        /// SPARQL endpoint URL (default: local store or http://localhost:7878/query)
        #[arg(short, long)]
        endpoint: Option<String>,
    },

    /// View in-binary documentation guides and tested SPARQL query recipes
    Moreinfo {
        /// Documentation topic (e.g. recipes, closures, diamond, cycles, scan-guard, prefixes)
        topic: Option<String>,

        /// Emit structured JSON for LLM agent tools
        #[arg(long)]
        json: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = config::Config::load()?;
    config.ensure_dirs()?;

    // Lazy background sync of forx-index on every command (non-blocking)
    // Skip if user is running sync explicitly to avoid concurrent pulls
    if !matches!(cli.command, Commands::Sync) {
        index::sync_background(&config)?;
    }

    match cli.command {
        Commands::Sync => {
            index::sync(&config)?;
            catalog::generate(&config)?;
        }

        Commands::Repos { query } => {
            let is_parsed_status = |status: &str| matches!(status, "parsed" | "ast_complete" | "enrich_complete");
            let all_repos = index::list_repos(&config)?;

            if let Some(q) = query {
                // If it looks like org/repo, show detail view
                if q.contains('/') {
                    let (org, name) = parse_repo(&q)?;
                    let manifest = index::read_repo_manifest(&config, org, name)?;
                    let cache_dir = config.cache_path(org, name);

                    let parsed_count = manifest.tracked_commits.iter()
                        .filter(|c| is_parsed_status(&c.parse_status)).count();
                    let pending_count = manifest.tracked_commits.iter()
                        .filter(|c| !is_parsed_status(&c.parse_status)).count();

                    println!("{}/{}", org, name);
                    println!("  {} parsed, {} pending\n", parsed_count, pending_count);

                    // Parsed commits with cache status
                    let mut parsed: Vec<_> = manifest.tracked_commits.iter()
                        .filter(|c| is_parsed_status(&c.parse_status))
                        .collect();
                    parsed.sort_by(|a, b| a.parsed_at.cmp(&b.parsed_at));

                    if !parsed.is_empty() {
                        println!("  Parsed:");
                        for c in &parsed {
                            let cached = cache_dir.join(&c.hexsha).exists();
                            let cache_marker = if cached { " [cached]" } else { "" };
                            let tag = c.tag_name.as_deref().unwrap_or("(untagged)");

                            // Try to get graph file info for size
                            let size_info = if cached {
                                index::read_commit_manifest(&config, org, name, &c.hexsha)
                                    .ok()
                                    .map(|cm| {
                                        let total: u64 = cm.graph_files.iter().map(|g| g.graph_file_size).sum();
                                        let types: Vec<&str> = cm.graph_files.iter().map(|g| g.graph_type.as_str()).collect();
                                        format!("  ({:.1} MB: {})", total as f64 / 1_048_576.0, types.join(", "))
                                    })
                                    .unwrap_or_default()
                            } else {
                                index::read_commit_manifest(&config, org, name, &c.hexsha)
                                    .ok()
                                    .map(|cm| {
                                        let total: u64 = cm.graph_files.iter().map(|g| g.graph_file_size).sum();
                                        let types: Vec<&str> = cm.graph_files.iter().map(|g| g.graph_type.as_str()).collect();
                                        format!("  ({:.1} MB: {})", total as f64 / 1_048_576.0, types.join(", "))
                                    })
                                    .unwrap_or_default()
                            };

                            println!(
                                "    {}  {:<30} {}{}",
                                &c.hexsha[..8],
                                tag,
                                size_info,
                                cache_marker
                            );
                        }
                    }

                    let pending: Vec<_> = manifest.tracked_commits.iter()
                        .filter(|c| !is_parsed_status(&c.parse_status))
                        .collect();

                    if !pending.is_empty() {
                        println!("\n  Pending ({}):", pending.len());
                        for c in pending.iter().take(10) {
                            println!(
                                "    {}  {}",
                                &c.hexsha[..8],
                                c.tag_name.as_deref().unwrap_or("(untagged)")
                            );
                        }
                        if pending.len() > 10 {
                            println!("    ... and {} more", pending.len() - 10);
                        }
                    }

                    println!("\n  Download: rlex download {}/{} <tag>", org, name);
                } else {
                    // Search mode — fuzzy match on org/repo
                    let search = q.to_lowercase();
                    let matches: Vec<_> = all_repos
                        .iter()
                        .filter(|(org, repo)| {
                            let full = format!("{}/{}", org, repo).to_lowercase();
                            full.contains(&search)
                                || org.to_lowercase().contains(&search)
                                || repo.to_lowercase().contains(&search)
                        })
                        .collect();

                    if matches.is_empty() {
                        println!("No repos matching '{}'. {} repos in index.", q, all_repos.len());
                    } else {
                        println!("{} repos matching '{}':\n", matches.len(), q);
                        for (org, repo) in &matches {
                            let info = index::read_repo_manifest(&config, org, repo)
                                .ok()
                                .map(|m| {
                                    let parsed = m.tracked_commits.iter()
                                        .filter(|c| is_parsed_status(&c.parse_status)).count();
                                    let total = m.tracked_commits.len();
                                    let cached = config.cache_path(org, repo).exists()
                                        && std::fs::read_dir(config.cache_path(org, repo))
                                            .map(|d| d.count() > 0).unwrap_or(false);
                                    let cache_marker = if cached { " [cached]" } else { "" };
                                    format!("  ({}/{} parsed){}", parsed, total, cache_marker)
                                 })
                                .unwrap_or_default();
                            println!("  {}/{}{}", org, repo, info);
                        }
                    }
                }
            } else {
                // List all repos with summary stats
                println!("{} repositories in forx-index:\n", all_repos.len());
                for (org, repo) in &all_repos {
                    let info = index::read_repo_manifest(&config, org, repo)
                        .ok()
                        .map(|m| {
                            let parsed = m.tracked_commits.iter()
                                .filter(|c| is_parsed_status(&c.parse_status)).count();
                            let total = m.tracked_commits.len();
                            let cached = config.cache_path(org, repo).exists()
                                && std::fs::read_dir(config.cache_path(org, repo))
                                    .map(|d| d.count() > 0).unwrap_or(false);
                            let cache_marker = if cached { " [cached]" } else { "" };
                            format!("  ({}/{} parsed){}", parsed, total, cache_marker)
                        })
                        .unwrap_or_default();
                    println!("  {}/{}{}", org, repo, info);
                }
            }
        }

        Commands::Download { repo, target, graphs: _ } => {
            let (org, name) = parse_repo(&repo)?;
            let hexsha = download::run(&config, org, name, &target)?;
            // Auto-load into oxigraph after download
            load::run(&config, org, name, Some(&hexsha))?;
        }

        Commands::Cache => {
            let cached = download::list_cached(&config)?;
            if cached.is_empty() {
                println!("Cache is empty. Use `rlex download org/repo tag` to download data.");
                return Ok(());
            }

            println!("Cached data:\n");
            for repo in &cached {
                println!("  {}/{}:", repo.org, repo.repo);
                for commit in &repo.commits {
                    println!(
                        "    {} — {} files, {:.1} MB",
                        &commit.hexsha[..8.min(commit.hexsha.len())],
                        commit.file_count,
                        commit.total_size as f64 / 1_048_576.0
                    );
                }
            }
        }

        Commands::Load { repo, commit } => {
            let (org, name) = parse_repo(&repo)?;
            load::run(&config, org, name, commit.as_deref())?;
        }
        Commands::Query { sparql, format, no_union, strict_scan_guard } => {
            let query_str = if std::path::Path::new(&sparql).is_file() {
                std::fs::read_to_string(&sparql)?
            } else {
                sparql
            };
            query::run(&config, &query_str, &format, !no_union, strict_scan_guard)?;
        }
        Commands::Calls { from, to, hops, format, endpoint } => {
            calls::run(
                &config,
                &calls::CallsOptions {
                    from,
                    to,
                    hops,
                    format,
                    endpoint,
                },
            )?;
        }
        Commands::Closure { from, to, depth, format, endpoint } => {
            closure::run(
                &config,
                &closure::ClosureOptions {
                    from,
                    to,
                    max_depth: depth,
                    format,
                    endpoint,
                },
            )?;
        }
        Commands::Diamond { from, to, format, endpoint } => {
            diamond::run(
                &config,
                &diamond::DiamondOptions {
                    from,
                    to,
                    format,
                    endpoint,
                },
            )?;
        }
        Commands::Cycles { repo, format, endpoint } => {
            cycles::run(
                &config,
                &cycles::CyclesOptions {
                    repo,
                    format,
                    endpoint,
                },
            )?;
        }
        Commands::Warmup { json, endpoint } => {
            warmup::run(&config, endpoint.as_deref(), json)?;
        }
        Commands::Moreinfo { topic, json } => {
            moreinfo::run(topic.as_deref(), json)?;
        }
        Commands::Audit {
            repo_pos,
            repo,
            commit,
            format,
            json,
            endpoint,
        } => {
            let target_repo = repo.or(repo_pos);
            if let Some(target) = target_repo {
                let fmt = if json {
                    "json"
                } else {
                    format.as_deref().unwrap_or("markdown")
                };
                audit::run_repo_audit(&config, &target, commit.as_deref(), fmt, endpoint.as_deref())?;
            } else {
                audit::run(&config, json, endpoint.as_deref())?;
            }
        }
        Commands::Bench { endpoint, iterations, suite, json } => {
            bench::run(&config, endpoint.as_deref(), iterations, &suite, json)?;
        }
        Commands::Serve { port, viz_dir, stop, foreground, no_browser } => {
            if stop {
                serve::stop(&config)?;
            } else if foreground {
                serve::run(&config, port, viz_dir.as_deref())?;
            } else {
                serve::background(&config, port, viz_dir.as_deref(), !no_browser)?;
            }
        }
        Commands::Viz { port, sparql_port, stop } => { viz::run(&config, port, sparql_port, stop)?; }
        Commands::Config => {
            println!("{}", toml::to_string_pretty(&config)?);
        }
    }

    Ok(())
}

fn parse_repo(s: &str) -> Result<(&str, &str)> {
    s.split_once('/')
        .ok_or_else(|| anyhow::anyhow!("Expected org/repo format, got '{}'", s))
}
