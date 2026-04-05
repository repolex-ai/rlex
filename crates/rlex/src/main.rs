use clap::{Parser, Subcommand};
use anyhow::Result;

mod catalog;
mod config;
mod download;
mod index;
mod load;
mod query;
mod serve;
mod viz;

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
    },

    /// Start SPARQL HTTP endpoint + viz API + catalog API
    Serve {
        /// Port for HTTP server
        #[arg(short, long, default_value = "7878")]
        port: u16,
        /// Directory containing index.html for viz UI
        #[arg(long)]
        viz_dir: Option<String>,
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
            let all_repos = index::list_repos(&config)?;

            if let Some(q) = query {
                // If it looks like org/repo, show detail view
                if q.contains('/') {
                    let (org, name) = parse_repo(&q)?;
                    let manifest = index::read_repo_manifest(&config, org, name)?;
                    let cache_dir = config.cache_path(org, name);

                    let parsed_count = manifest.tracked_commits.iter()
                        .filter(|c| c.parse_status == "parsed").count();
                    let pending_count = manifest.tracked_commits.iter()
                        .filter(|c| c.parse_status != "parsed").count();

                    println!("{}/{}", org, name);
                    println!("  {} parsed, {} pending\n", parsed_count, pending_count);

                    // Parsed commits with cache status
                    let mut parsed: Vec<_> = manifest.tracked_commits.iter()
                        .filter(|c| c.parse_status == "parsed")
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
                        .filter(|c| c.parse_status != "parsed")
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
                                        .filter(|c| c.parse_status == "parsed").count();
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
                                .filter(|c| c.parse_status == "parsed").count();
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
        Commands::Query { sparql, format } => { query::run(&config, &sparql, &format)?; }
        Commands::Serve { port, viz_dir } => { serve::run(&config, port, viz_dir.as_deref())?; }
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
