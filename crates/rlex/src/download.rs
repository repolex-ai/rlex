use anyhow::{bail, Context, Result};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use crate::config::Config;
use crate::index::{self, GraphFile};

/// Download graph files for a specific commit of a repo.
///
/// Files land in ~/.rlex/cache/{org}/{repo}/{hexsha}/
/// Download URLs: https://github.com/repolex-forx/{org}--{repo}/raw/main/{graphFilePath}
/// Returns the hexsha of the downloaded commit.
pub fn run(config: &Config, org: &str, repo: &str, target: &str) -> Result<String> {
    // Read repo manifest to find the commit
    let repo_manifest = index::read_repo_manifest(config, org, repo)?;

    // Target can be a tag name or a commit sha (prefix match)
    let commit = repo_manifest
        .tracked_commits
        .iter()
        .find(|c| {
            c.tag_name.as_deref() == Some(target)
                || c.hexsha.starts_with(target)
        })
        .ok_or_else(|| anyhow::anyhow!(
            "No commit matching '{}' in {}/{}. Use `rlex repos {}/{}` to see available tags.",
            target, org, repo, org, repo
        ))?;

    if commit.parse_status != "parsed" {
        bail!(
            "Commit {} ({}) is '{}', not yet parsed. Cannot download.",
            &commit.hexsha[..8],
            commit.tag_name.as_deref().unwrap_or("untagged"),
            commit.parse_status
        );
    }

    // Read the commit manifest for file details
    let commit_manifest = index::read_commit_manifest(config, org, repo, &commit.hexsha)?;

    let cache_dir = config.cache_path(org, repo).join(&commit.hexsha);
    fs::create_dir_all(&cache_dir)
        .with_context(|| format!("creating cache dir {}", cache_dir.display()))?;

    // forx repo name uses -- separator: repolex-forx/{org}--{repo}
    let forx_repo = format!("{}--{}", org, repo);
    let base_url = format!(
        "https://github.com/repolex-forx/{}/raw/main",
        forx_repo
    );

    println!(
        "Downloading {}/{} @ {} ({})...",
        org,
        repo,
        &commit.hexsha[..8],
        commit.tag_name.as_deref().unwrap_or("untagged")
    );

    let client = reqwest::blocking::Client::new();
    let mut downloaded = 0u64;
    let mut skipped = 0;

    for graph_file in &commit_manifest.graph_files {
        let dest = cache_dir.join(&graph_file.graph_file_path);

        // Skip if already downloaded
        if dest.exists() {
            skipped += 1;
            continue;
        }

        let url = format!("{}/{}", base_url, graph_file.graph_file_path);
        download_file(&client, &url, &dest, &graph_file.graph_type, graph_file.graph_file_size)?;
        downloaded += graph_file.graph_file_size;
    }

    let total = commit_manifest.graph_files.len();
    println!(
        "Done. {} files downloaded ({:.1} MB), {} skipped (already cached).",
        total - skipped,
        downloaded as f64 / 1_048_576.0,
        skipped
    );
    println!("Cache: {}", cache_dir.display());

    Ok(commit.hexsha.clone())
}

fn download_file(
    client: &reqwest::blocking::Client,
    url: &str,
    dest: &PathBuf,
    graph_type: &str,
    expected_size: u64,
) -> Result<()> {
    // Ensure parent dirs exist (e.g. filetree/, commit/, etc.)
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }

    print!("  {} ({:.1} MB)... ", graph_type, expected_size as f64 / 1_048_576.0);
    std::io::stdout().flush()?;

    let response = client
        .get(url)
        .send()
        .with_context(|| format!("fetching {}", url))?;

    if !response.status().is_success() {
        println!("FAILED ({})", response.status());
        bail!("Download failed for {}: HTTP {}", url, response.status());
    }

    let bytes = response.bytes()
        .with_context(|| format!("reading response from {}", url))?;

    fs::write(dest, &bytes)
        .with_context(|| format!("writing {}", dest.display()))?;

    println!("ok");
    Ok(())
}

/// List what's currently downloaded in the cache
pub fn list_cached(config: &Config) -> Result<Vec<CachedRepo>> {
    let cache_dir = &config.paths.cache;
    if !cache_dir.exists() {
        return Ok(Vec::new());
    }

    let mut cached = Vec::new();

    for org_entry in fs::read_dir(cache_dir)? {
        let org_entry = org_entry?;
        if !org_entry.file_type()?.is_dir() {
            continue;
        }
        let org = org_entry.file_name().to_string_lossy().to_string();

        for repo_entry in fs::read_dir(org_entry.path())? {
            let repo_entry = repo_entry?;
            if !repo_entry.file_type()?.is_dir() {
                continue;
            }
            let repo = repo_entry.file_name().to_string_lossy().to_string();

            let mut commits = Vec::new();
            for commit_entry in fs::read_dir(repo_entry.path())? {
                let commit_entry = commit_entry?;
                if !commit_entry.file_type()?.is_dir() {
                    continue;
                }
                let sha = commit_entry.file_name().to_string_lossy().to_string();

                // Count files and total size
                let mut file_count = 0u32;
                let mut total_size = 0u64;
                for entry in walkdir::WalkDir::new(commit_entry.path()).into_iter().flatten() {
                    if entry.file_type().is_file() {
                        file_count += 1;
                        if let Ok(meta) = entry.metadata() {
                            total_size += meta.len();
                        }
                    }
                }

                commits.push(CachedCommit {
                    hexsha: sha,
                    file_count,
                    total_size,
                });
            }

            if !commits.is_empty() {
                cached.push(CachedRepo {
                    org: org.clone(),
                    repo,
                    commits,
                });
            }
        }
    }

    cached.sort_by(|a, b| format!("{}/{}", a.org, a.repo).cmp(&format!("{}/{}", b.org, b.repo)));
    Ok(cached)
}

pub struct CachedRepo {
    pub org: String,
    pub repo: String,
    pub commits: Vec<CachedCommit>,
}

pub struct CachedCommit {
    pub hexsha: String,
    pub file_count: u32,
    pub total_size: u64,
}
