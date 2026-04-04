use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use oxigraph::io::RdfFormat;
use oxigraph::store::Store;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::Path;
use std::time::Instant;

use crate::config::Config;

/// Load cached .nq.gz files for a repo/commit into the oxigraph store.
///
/// If a specific commit sha is given, loads just that commit.
/// Otherwise loads all cached commits for the repo.
pub fn run(config: &Config, org: &str, repo: &str, commit: Option<&str>) -> Result<()> {
    let repo_cache = config.cache_path(org, repo);
    if !repo_cache.exists() {
        bail!(
            "No cached data for {}/{}. Run `rlex download {}/{} <tag>` first.",
            org, repo, org, repo
        );
    }

    let store = Store::open(&config.paths.oxigraph)
        .with_context(|| format!("opening oxigraph store at {}", config.paths.oxigraph.display()))?;

    let commits_to_load: Vec<String> = if let Some(sha) = commit {
        // Find matching commit dir (prefix match)
        let matching = find_commit_dir(&repo_cache, sha)?;
        vec![matching]
    } else {
        // Load all cached commits
        let mut dirs = Vec::new();
        for entry in fs::read_dir(&repo_cache)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                dirs.push(entry.file_name().to_string_lossy().to_string());
            }
        }
        dirs.sort();
        dirs
    };

    for sha in &commits_to_load {
        let commit_dir = repo_cache.join(sha);
        load_commit(&store, &commit_dir, org, repo, sha)?;
    }

    println!("\nStore: {}", config.paths.oxigraph.display());
    Ok(())
}

fn load_commit(store: &Store, commit_dir: &Path, org: &str, repo: &str, sha: &str) -> Result<()> {
    println!("Loading {}/{} @ {}...", org, repo, &sha[..8.min(sha.len())]);

    let mut files_loaded = 0u32;
    let mut total_triples = 0u64;
    let start = Instant::now();

    // Walk the commit dir for .nq.gz files
    for entry in walkdir::WalkDir::new(commit_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let name = path.file_name().unwrap().to_string_lossy();
        if name.ends_with(".nq.gz") {
            let graph_type = path
                .parent()
                .and_then(|p| p.file_name())
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default();

            print!("  {} ({})... ", name, graph_type);

            let count = load_nquads_gz(store, path)?;
            total_triples += count as u64;
            files_loaded += 1;

            println!("{} quads", count);
        } else if name.ends_with(".nq") {
            let graph_type = path
                .parent()
                .and_then(|p| p.file_name())
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default();

            print!("  {} ({})... ", name, graph_type);

            let count = load_nquads(store, path)?;
            total_triples += count as u64;
            files_loaded += 1;

            println!("{} quads", count);
        }
    }

    let elapsed = start.elapsed();
    println!(
        "  Loaded {} files, {} quads in {:.1}s",
        files_loaded, total_triples, elapsed.as_secs_f64()
    );

    Ok(())
}

fn load_nquads_gz(store: &Store, path: &Path) -> Result<usize> {
    let file = File::open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    let decoder = GzDecoder::new(BufReader::new(file));

    let mut loader = store.bulk_loader();
    loader
        .load_from_reader(RdfFormat::NQuads, decoder)
        .with_context(|| format!("loading {}", path.display()))?;
    loader.commit()
        .with_context(|| format!("committing {}", path.display()))?;

    Ok(0)
}

fn load_nquads(store: &Store, path: &Path) -> Result<usize> {
    let file = File::open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    let reader = BufReader::new(file);

    let mut loader = store.bulk_loader();
    loader
        .load_from_reader(RdfFormat::NQuads, reader)
        .with_context(|| format!("loading {}", path.display()))?;
    loader.commit()
        .with_context(|| format!("committing {}", path.display()))?;

    Ok(0)
}

fn find_commit_dir(repo_cache: &Path, prefix: &str) -> Result<String> {
    for entry in fs::read_dir(repo_cache)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(prefix) {
            return Ok(name);
        }
    }
    bail!("No cached commit matching '{}' found", prefix);
}
