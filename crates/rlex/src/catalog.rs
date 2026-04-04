use anyhow::{Context, Result};
use serde::Serialize;
use std::fs;

use crate::config::Config;
use crate::index;

/// Generate ~/.rlex/catalog.json from forx-index data
pub fn generate(config: &Config) -> Result<()> {
    let repos = index::list_repos(config)?;
    let mut catalog_repos = Vec::new();

    for (org, repo) in &repos {
        match index::read_repo_manifest(config, org, repo) {
            Ok(manifest) => {
                let mut commits = Vec::new();
                let mut parsed_count = 0u32;
                let mut pending_count = 0u32;

                for tc in &manifest.tracked_commits {
                    let is_parsed = tc.parse_status == "parsed";
                    if is_parsed {
                        parsed_count += 1;
                    } else {
                        pending_count += 1;
                    }

                    // Check if this commit is cached locally
                    let cached = config
                        .cache_path(org, repo)
                        .join(&tc.hexsha)
                        .exists();

                    // Try to read commit manifest for graph file details
                    let graph_files = if is_parsed {
                        index::read_commit_manifest(config, org, repo, &tc.hexsha)
                            .ok()
                            .map(|cm| {
                                cm.graph_files
                                    .iter()
                                    .map(|gf| CatalogGraphFile {
                                        graph_type: gf.graph_type.clone(),
                                        size_bytes: gf.graph_file_size,
                                    })
                                    .collect()
                            })
                    } else {
                        None
                    };

                    commits.push(CatalogCommit {
                        hexsha: tc.hexsha.clone(),
                        tag: tc.tag_name.clone(),
                        status: tc.parse_status.clone(),
                        parsed_at: tc.parsed_at.clone(),
                        cached,
                        graph_files,
                    });
                }

                catalog_repos.push(CatalogRepo {
                    org: org.clone(),
                    repo: repo.clone(),
                    parsed_count,
                    pending_count,
                    commits,
                });
            }
            Err(_) => continue,
        }
    }

    let catalog = Catalog {
        version: 1,
        repo_count: catalog_repos.len() as u32,
        repos: catalog_repos,
    };

    let catalog_path = config.paths.root.join("catalog.json");
    let json = serde_json::to_string_pretty(&catalog)
        .context("serializing catalog")?;
    fs::write(&catalog_path, &json)
        .with_context(|| format!("writing {}", catalog_path.display()))?;

    println!("Catalog written to {} ({} repos)", catalog_path.display(), catalog.repo_count);

    Ok(())
}

#[derive(Debug, Serialize)]
pub struct Catalog {
    pub version: u32,
    pub repo_count: u32,
    pub repos: Vec<CatalogRepo>,
}

#[derive(Debug, Serialize)]
pub struct CatalogRepo {
    pub org: String,
    pub repo: String,
    pub parsed_count: u32,
    pub pending_count: u32,
    pub commits: Vec<CatalogCommit>,
}

#[derive(Debug, Serialize)]
pub struct CatalogCommit {
    pub hexsha: String,
    pub tag: Option<String>,
    pub status: String,
    pub parsed_at: Option<String>,
    pub cached: bool,
    pub graph_files: Option<Vec<CatalogGraphFile>>,
}

#[derive(Debug, Serialize)]
pub struct CatalogGraphFile {
    pub graph_type: String,
    pub size_bytes: u64,
}
