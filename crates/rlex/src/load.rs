use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use oxigraph::io::RdfFormat;
use oxigraph::store::Store;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
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
    let mut total_quads = 0u64;
    let mut failures: Vec<(String, String)> = Vec::new();
    let start = Instant::now();

    // Walk the commit dir for .nq.gz / .nq files
    for entry in walkdir::WalkDir::new(commit_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let name = path.file_name().unwrap().to_string_lossy();
        let gzipped = name.ends_with(".nq.gz");
        if !gzipped && !name.ends_with(".nq") {
            continue;
        }

        let graph_type = path
            .parent()
            .and_then(|p| p.file_name())
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();

        print!("  {} ({})... ", name, graph_type);

        // Load each file independently. A parse error in one graph — e.g. an
        // invalid IRI in commit/issue/pr metadata from an unescaped GitHub
        // "[bot]" username — must NOT sink the whole repo: the AST and symbol
        // data an agent actually queries lives in other files. We skip the bad
        // file LOUDLY (never silently) and keep going, then summarize.
        let result = if gzipped {
            load_nquads_gz(store, path)
        } else {
            load_nquads(store, path)
        };
        match result {
            Ok(()) => {
                // Count statements from the file itself (one N-Quad per line),
                // NOT via store.len() — on a large persistent store len() scans
                // everything, which made loading O(store size) per file.
                let count = count_statements(path, gzipped).unwrap_or(0);
                total_quads += count;
                files_loaded += 1;
                println!("{} quads", count);
            }
            Err(e) => {
                // Unwrap the anyhow chain to the deepest cause for a tight message
                // (the actual parser error, not our "loading <path>" wrapper).
                let cause = e.root_cause().to_string();
                println!("SKIPPED — {}", cause);
                failures.push((format!("{}/{}", graph_type, name), cause));
            }
        }
    }

    let elapsed = start.elapsed();
    println!(
        "  Loaded {} files, {} quads in {:.1}s",
        files_loaded, total_quads, elapsed.as_secs_f64()
    );

    if !failures.is_empty() {
        eprintln!(
            "  ⚠ {} file(s) SKIPPED due to parse errors (data bug — report upstream):",
            failures.len()
        );
        for (file, cause) in &failures {
            eprintln!("      {} — {}", file, cause);
        }
        if files_loaded == 0 {
            bail!("no files loaded for {}/{} — every graph file failed to parse", org, repo);
        }
    }

    Ok(())
}

/// Count RDF statements in an N-Quads file: one statement per non-blank,
/// non-comment line. O(file), independent of store size. Slightly overcounts
/// if the file contains duplicate lines (RDF set semantics would collapse
/// them), which N-Quads exports do not do in practice.
fn count_statements(path: &Path, gzipped: bool) -> Result<u64> {
    let file = File::open(path)?;
    let reader: Box<dyn std::io::BufRead> = if gzipped {
        Box::new(BufReader::new(GzDecoder::new(BufReader::new(file))))
    } else {
        Box::new(BufReader::new(file))
    };
    let mut n = 0u64;
    for line in reader.lines() {
        let line = line?;
        let t = line.trim_start();
        if !t.is_empty() && !t.starts_with('#') {
            n += 1;
        }
    }
    Ok(n)
}

fn load_nquads_gz(store: &Store, path: &Path) -> Result<()> {
    let file = File::open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    let decoder = GzDecoder::new(BufReader::new(file));

    let mut loader = store.bulk_loader();
    loader
        .load_from_reader(RdfFormat::NQuads, decoder)
        .with_context(|| format!("loading {}", path.display()))?;
    loader.commit()
        .with_context(|| format!("committing {}", path.display()))?;

    Ok(())
}

fn load_nquads(store: &Store, path: &Path) -> Result<()> {
    let file = File::open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    let reader = BufReader::new(file);

    let mut loader = store.bulk_loader();
    loader
        .load_from_reader(RdfFormat::NQuads, reader)
        .with_context(|| format!("loading {}", path.display()))?;
    loader.commit()
        .with_context(|| format!("committing {}", path.display()))?;

    Ok(())
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
