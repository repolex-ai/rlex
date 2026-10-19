use anyhow::{bail, Context, Result};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use crate::config::Config;
use crate::index;

/// Download graph files for a specific commit of a repo.
///
/// Files land in ~/.rlex/cache/{org}/{repo}/{hexsha}/
/// Download URLs: https://github.com/repolex-forx/{org}--{repo}/raw/main/{graphFilePath}
/// Returns the hexsha of the downloaded commit.
pub fn run(config: &Config, org: &str, repo: &str, target: &str) -> Result<String> {
    // Read repo manifest to find the commit
    let repo_manifest = index::read_repo_manifest(config, org, repo)?;

    // Target can be "HEAD" (latest parsed commit), a tag name, or a commit sha (prefix match)
    let is_parsed = |s: &str| matches!(s, "parsed" | "ast_complete" | "enrich_complete");
    let commit = if target.eq_ignore_ascii_case("HEAD") {
        repo_manifest
            .tracked_commits
            .iter()
            .filter(|c| is_parsed(&c.parse_status))
            .max_by_key(|c| c.parsed_at.as_deref().unwrap_or(""))
            .or_else(|| repo_manifest.tracked_commits.first())
    } else {
        repo_manifest
            .tracked_commits
            .iter()
            .find(|c| {
                c.tag_name.as_deref() == Some(target)
                    || c.hexsha.starts_with(target)
            })
    }
    .ok_or_else(|| anyhow::anyhow!(
        "No commit matching '{}' in {}/{}. Use `rlex repos {}/{}` to see available tags.",
        target, org, repo, org, repo
    ))?;

    if commit.parse_status != "parsed"
        && commit.parse_status != "ast_complete"
        && commit.parse_status != "enrich_complete"
    {
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

    // storage repo name uses -- separator: {data_storage_org}/{org}--{repo}
    let forx_repo = format!("{}--{}", org, repo);
    let base_url = format!(
        "https://github.com/{}/{}/raw/main",
        config.data_storage_org, forx_repo
    );
    let repo_clone_dir = config.paths.repos.join(&config.data_storage_org).join(&forx_repo);

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
        if download_file(&client, &url, &dest, &graph_file.graph_type, graph_file.graph_file_size).is_err() {
            copy_from_git_clone(&config.data_storage_org, &forx_repo, &repo_clone_dir, &graph_file.graph_file_path, &dest)?;
        }
        downloaded += graph_file.graph_file_size;
    }

    // Fallback: Check if aggregate graphs exist in the storage repo even if omitted from manifest
    let has_ast = commit_manifest.graph_files.iter().any(|g| g.graph_type == "ast");
    let mut total = commit_manifest.graph_files.len();
    if !has_ast {
        let probe_paths = [
            (format!("aggregate/ast/{}/chunk-001.nq.gz", commit.hexsha), "ast"),
            (format!("aggregate/lsp/{}.nq.gz", commit.hexsha), "lsp"),
            (format!("aggregate/repolex/{}/chunk-001.nq.gz", commit.hexsha), "repolex"),
        ];

        for (rel_path, g_type) in probe_paths {
            let dest = cache_dir.join(&rel_path);
            if dest.exists() {
                total += 1;
                skipped += 1;
                continue;
            }
            let url = format!("{}/{}", base_url, rel_path);
            if let Ok(resp) = client.head(&url).send() {
                if resp.status().is_success() {
                    total += 1;
                    let content_len = resp.headers()
                        .get(reqwest::header::CONTENT_LENGTH)
                        .and_then(|h| h.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(0);
                    if download_file(&client, &url, &dest, g_type, content_len).is_ok() {
                        downloaded += content_len;
                    }
                    continue;
                }
            }
            // Also check local git clone if private repo was cloned
            let src = repo_clone_dir.join(&rel_path);
            if src.exists() {
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
                if let Ok(bytes) = fs::copy(&src, &dest) {
                    total += 1;
                    downloaded += bytes;
                }
            }
        }
    }

    println!(
        "Done. {} files downloaded ({:.1} MB), {} skipped (already cached).",
        total.saturating_sub(skipped),
        downloaded as f64 / 1_048_576.0,
        skipped
    );
    println!("Cache: {}", cache_dir.display());

    Ok(commit.hexsha.clone())
}

fn copy_from_git_clone(
    data_storage_org: &str,
    forx_repo: &str,
    repo_clone_dir: &PathBuf,
    rel_path: &str,
    dest: &PathBuf,
) -> Result<()> {
    if !repo_clone_dir.join(".git").exists() {
        if let Some(parent) = repo_clone_dir.parent() {
            fs::create_dir_all(parent)?;
        }
        let repo_url = format!("https://github.com/{}/{}.git", data_storage_org, forx_repo);
        let output = std::process::Command::new("git")
            .args(["clone", "--depth", "1", &repo_url, &repo_clone_dir.to_string_lossy()])
            .output()
            .context("running git clone for data repo")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git clone failed: {}", stderr.trim());
        }
    } else {
        let _ = std::process::Command::new("git")
            .args(["pull", "--ff-only"])
            .current_dir(repo_clone_dir)
            .output();
    }
    let src = repo_clone_dir.join(rel_path);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(&src, dest)
        .with_context(|| format!("copying {} from git clone", rel_path))?;
    println!("ok (via git)");
    Ok(())
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
