use std::path::Path;

#[derive(Debug, Clone)]
pub struct RepoRef {
    pub org: String,
    pub repo: String,
    pub commit: String,
}

impl RepoRef {
    pub fn lsp_graph(&self) -> String {
        format!("https://repolex.ai/r/{}/{}/lsp/{}", self.org, self.repo, self.commit)
    }

    #[allow(dead_code)]
    pub fn dep_graph(&self) -> String {
        format!("https://repolex.ai/r/{}/{}/dep/{}", self.org, self.repo, self.commit)
    }

    #[allow(dead_code)]
    pub fn ast_graph(&self) -> String {
        format!("https://repolex.ai/r/{}/{}/ast/{}", self.org, self.repo, self.commit)
    }
}

const BACKBONE: &[(&str, &str, &str, &str)] = &[
    ("rlex", "repolex-ai", "rlex", "550a8e5a1a7b121bd970eff3e7575acd158f6bb8"),
    ("git-lex", "repolex-ai", "git-lex", "d79791e26e44dbd3667f05caa6a97fcd12b3d146"),
    ("actix", "actix", "actix-web", "5723cf486522d47aad26390cf5b02e95654ae225"),
    ("actix-web", "actix", "actix-web", "5723cf486522d47aad26390cf5b02e95654ae225"),
    ("regex", "rust-lang", "regex", "25a15e272b3ae5aee76b525902c2ab91b0d9e12e"),
    ("aho-corasick", "BurntSushi", "aho-corasick", "d84a5073d5108fce1774b375105dfdb13fe4e81c"),
    ("memchr", "BurntSushi", "memchr", "886ca4ca4820297191c6e9f7b023dc356f31a4d1"),
    ("walkdir", "BurntSushi", "walkdir", "4f26be4d450910916ea11533b2efc52b9a6483bc"),
    ("axum", "tokio-rs", "axum", "c59208c86fded335cd85e388030ad59347b0e5ae"),
    ("bytes", "tokio-rs", "bytes", "417dccdeff249e0c011327de7d92e0d6fbe7cc43"),
    ("syn", "dtolnay", "syn", "7bcb37cdb3399977658c8b52d2441d37e42e48f2"),
    ("quote", "dtolnay", "quote", "842ffde933fdd76cd1681a288bed136d8b95a97a"),
    ("proc-macro2", "dtolnay", "proc-macro2", "58ab776b95a4c2865554badbb6629c50971a9118"),
    ("anyhow", "dtolnay", "anyhow", "5c657b32522023a9f7ef883fb08582fd8e656b1a"),
    ("serde", "serde-rs", "serde", "7fc3b4c30c94f73a96ebd1553f2b090d928fc3a8"),
    ("serde-json", "serde-rs", "json", "4f6dbfac79647d032b0997b5ab73022340c6dab7"),
    ("socket2", "rust-lang", "socket2", "239dd83a4ced08e514d2c38942aab99791119f0d"),
    ("libc", "rust-lang", "libc", "f3417ae86f015b9d0d8789b8fa11e64f9fec0adf"),
    ("git2", "rust-lang", "git2-rs", "b863968301f0e889fa04afc590d7e2c9a4100dc3"),
    ("git2-rs", "rust-lang", "git2-rs", "b863968301f0e889fa04afc590d7e2c9a4100dc3"),
    ("flate2", "rust-lang", "flate2-rs", "93c81772305a102f1ec846bd12713dd7bf1e3f04"),
    ("flate2-rs", "rust-lang", "flate2-rs", "93c81772305a102f1ec846bd12713dd7bf1e3f04"),
    ("miniz_oxide", "Frommi", "miniz_oxide", "86d92e8d284b0acd451e7b6db6bdef41ae9c9db4"),
    ("crc32fast", "srijs", "rust-crc32fast", "a150f65ce810793293d5c9dd815f4510eb6d8e4c"),
    ("http", "hyperium", "http", "b9625d83b524f7a8306883484f29a746eefc1bab"),
    ("hyper", "hyperium", "hyper", "0d6c7d5469baa09e2fb127ee3758a79b3271a4f0"),
    ("reqwest", "seanmonstar", "reqwest", "ad83b63824385a4e5758d263db707549bbe59ba7"),
    ("ruby", "ruby", "ruby", "995b59f66677d44767ce9faac6957e5543617ff9"),
    ("jquery", "jquery", "jquery", "4dec426aa2a6cbabb1b064319ba7c272d594a688"),
];

/// Resolve a repository query string (e.g. "syn", "regex", "dtolnay/syn")
/// to a RepoRef using known backbone mappings and local cache fallbacks.
pub fn resolve_repo(query: &str, cache_root: Option<&Path>) -> Option<RepoRef> {
    let q = query.trim().to_lowercase();
    let clean = q.replace("-rs", "").replace("_rs", "");

    // 1. Check backbone table
    for (alias, org, repo, commit) in BACKBONE {
        if q == *alias || clean == *alias || q == format!("{}/{}", org, repo).to_lowercase() {
            return Some(RepoRef {
                org: org.to_string(),
                repo: repo.to_string(),
                commit: commit.to_string(),
            });
        }
    }

    // 2. Partial match on backbone
    for (alias, org, repo, commit) in BACKBONE {
        if alias.contains(&clean) || clean.contains(alias) {
            return Some(RepoRef {
                org: org.to_string(),
                repo: repo.to_string(),
                commit: commit.to_string(),
            });
        }
    }

    // 3. Dynamic search in ~/.rlex/cache if path provided
    if let Some(cache_dir) = cache_root
        && cache_dir.exists()
            && let Ok(org_entries) = std::fs::read_dir(cache_dir) {
                for org_entry in org_entries.flatten() {
                    let org_name = org_entry.file_name().to_string_lossy().to_string();
                    if let Ok(repo_entries) = std::fs::read_dir(org_entry.path()) {
                        for repo_entry in repo_entries.flatten() {
                            let repo_name = repo_entry.file_name().to_string_lossy().to_string();
                            let full_name = format!("{}/{}", org_name, repo_name).to_lowercase();
                            if full_name.contains(&clean) || repo_name.to_lowercase().contains(&clean) {
                                // Find commit dir
                                if let Ok(commits) = std::fs::read_dir(repo_entry.path()) {
                                    for c in commits.flatten() {
                                        if c.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                                            let commit_sha = c.file_name().to_string_lossy().to_string();
                                            return Some(RepoRef {
                                                org: org_name,
                                                repo: repo_name,
                                                commit: commit_sha,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

    None
}

/// Get all backbone repositories
pub fn get_backbone() -> Vec<RepoRef> {
    let mut seen = std::collections::HashSet::new();
    let mut repos = Vec::new();
    for (_, org, repo, commit) in BACKBONE {
        let key = format!("{}/{}", org, repo);
        if seen.insert(key) {
            repos.push(RepoRef {
                org: org.to_string(),
                repo: repo.to_string(),
                commit: commit.to_string(),
            });
        }
    }
    repos
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_exact_backbone() {
        let repo = resolve_repo("regex", None).expect("regex should resolve");
        assert_eq!(repo.org, "rust-lang");
        assert_eq!(repo.repo, "regex");
        assert_eq!(repo.commit, "25a15e272b3ae5aee76b525902c2ab91b0d9e12e");
        assert!(repo.lsp_graph().contains("rust-lang/regex/lsp/"));
    }

    #[test]
    fn test_resolve_with_owner() {
        let repo = resolve_repo("dtolnay/syn", None).expect("dtolnay/syn should resolve");
        assert_eq!(repo.org, "dtolnay");
        assert_eq!(repo.repo, "syn");
    }
}
