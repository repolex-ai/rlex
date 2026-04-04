use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::Config;

const FORX_INDEX_URL: &str = "https://github.com/repolex-forx/forx-index.git";

/// Clone or pull the forx-index repo
pub fn sync(config: &Config) -> Result<()> {
    let index_path = config.forx_index_path();

    if index_path.join(".git").exists() {
        println!("Pulling forx-index...");
        let output = Command::new("git")
            .args(["pull", "--ff-only"])
            .current_dir(&index_path)
            .output()
            .context("running git pull")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git pull failed: {}", stderr.trim());
        }
        println!("forx-index up to date.");
    } else {
        println!("Cloning forx-index...");
        fs::create_dir_all(index_path.parent().unwrap())
            .context("creating repos directory")?;

        let output = Command::new("git")
            .args(["clone", FORX_INDEX_URL, &index_path.to_string_lossy()])
            .output()
            .context("running git clone")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git clone failed: {}", stderr.trim());
        }
        println!("forx-index cloned to {}", index_path.display());
    }

    Ok(())
}

/// Spawn a non-blocking background sync of forx-index
pub fn sync_background(config: &Config) -> Result<()> {
    let index_path = config.forx_index_path();

    if !index_path.join(".git").exists() {
        // First time — must clone synchronously
        sync(config)?;
        return Ok(());
    }

    // Background pull — fire and forget
    let _child = Command::new("git")
        .args(["pull", "--ff-only"])
        .current_dir(&index_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("spawning background git pull")?;

    Ok(())
}

// -- Manifest data structures --

#[derive(Debug, Deserialize)]
pub struct RepoManifest {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "gh:name")]
    pub name: String,
    #[serde(rename = "gh:owner")]
    pub owner: String,
    #[serde(rename = "repolex:trackedCommit")]
    pub tracked_commits: Vec<TrackedCommit>,
}

#[derive(Debug, Deserialize)]
pub struct TrackedCommit {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "git:hexsha")]
    pub hexsha: String,
    #[serde(rename = "repolex:parseStatus")]
    pub parse_status: String,
    #[serde(rename = "git:tagName")]
    pub tag_name: Option<String>,
    #[serde(rename = "repolex:parsedAt")]
    pub parsed_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CommitManifest {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "git:hexsha")]
    pub hexsha: String,
    #[serde(rename = "repolex:graphFile")]
    pub graph_files: Vec<GraphFile>,
    #[serde(rename = "repolex:parseStatus")]
    pub parse_status: String,
    #[serde(rename = "git:tagName")]
    pub tag_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GraphFile {
    #[serde(rename = "repolex:graphType")]
    pub graph_type: String,
    #[serde(rename = "repolex:graphFilePath")]
    pub graph_file_path: String,
    #[serde(rename = "repolex:graphFileSize")]
    pub graph_file_size: u64,
}

// -- Index queries --

/// List all repos available in the forx-index
pub fn list_repos(config: &Config) -> Result<Vec<(String, String)>> {
    let repos_dir = config.forx_index_path().join("repos");
    if !repos_dir.exists() {
        bail!("forx-index not found. Run `rlex sync` first.");
    }

    let mut repos = Vec::new();
    for org_entry in fs::read_dir(&repos_dir)? {
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
            repos.push((org.clone(), repo));
        }
    }

    repos.sort();
    Ok(repos)
}

/// Read a repo manifest from the forx-index
pub fn read_repo_manifest(config: &Config, org: &str, repo: &str) -> Result<RepoManifest> {
    let manifest_path = config
        .forx_index_path()
        .join("repos")
        .join(org)
        .join(repo)
        .join("repo-manifest.jsonld");

    if !manifest_path.exists() {
        bail!("No manifest found for {}/{}. Run `rlex sync` to update the index.", org, repo);
    }

    let contents = fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let manifest: RepoManifest = serde_json::from_str(&contents)
        .with_context(|| format!("parsing {}", manifest_path.display()))?;

    Ok(manifest)
}

/// Read a commit manifest from the forx-index
pub fn read_commit_manifest(config: &Config, org: &str, repo: &str, hexsha: &str) -> Result<CommitManifest> {
    let manifest_path = config
        .forx_index_path()
        .join("repos")
        .join(org)
        .join(repo)
        .join("commits")
        .join(format!("commit-manifest-{}.jsonld", hexsha));

    if !manifest_path.exists() {
        bail!(
            "No commit manifest for {}/{} @ {}. Commit may not be parsed yet.",
            org, repo, &hexsha[..8.min(hexsha.len())]
        );
    }

    let contents = fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let manifest: CommitManifest = serde_json::from_str(&contents)
        .with_context(|| format!("parsing {}", manifest_path.display()))?;

    Ok(manifest)
}

/// Find a commit by tag name in a repo manifest
pub fn find_commit_by_tag(manifest: &RepoManifest, tag: &str) -> Option<TrackedCommit> {
    manifest
        .tracked_commits
        .iter()
        .find(|c| c.tag_name.as_deref() == Some(tag))
        .cloned()
}

// TrackedCommit needs Clone for find_commit_by_tag
impl Clone for TrackedCommit {
    fn clone(&self) -> Self {
        TrackedCommit {
            id: self.id.clone(),
            hexsha: self.hexsha.clone(),
            parse_status: self.parse_status.clone(),
            tag_name: self.tag_name.clone(),
            parsed_at: self.parsed_at.clone(),
        }
    }
}
