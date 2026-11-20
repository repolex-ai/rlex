use std::path::Path;

use crate::client::Client;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ecosystem {
    Cargo,
    Pypi,
    Npm,
    Go,
    RubyGems,
    Maven,
}

impl Ecosystem {
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "cargo" | "rust" | "crates.io" => Some(Self::Cargo),
            "pypi" | "python" | "pip" => Some(Self::Pypi),
            "npm" | "javascript" | "typescript" | "js" | "ts" => Some(Self::Npm),
            "go" | "golang" => Some(Self::Go),
            "rubygems" | "ruby" | "gem" => Some(Self::RubyGems),
            "maven" | "java" | "gradle" => Some(Self::Maven),
            _ => None,
        }
    }

    pub fn from_file_path(path: &str) -> Option<Self> {
        let lower = path.trim().to_ascii_lowercase();
        if lower.ends_with(".rs") {
            Some(Self::Cargo)
        } else if lower.ends_with(".py") || lower.ends_with(".pyi") {
            Some(Self::Pypi)
        } else if lower.ends_with(".ts")
            || lower.ends_with(".tsx")
            || lower.ends_with(".js")
            || lower.ends_with(".jsx")
            || lower.ends_with(".mjs")
            || lower.ends_with(".cjs")
        {
            Some(Self::Npm)
        } else if lower.ends_with(".go") {
            Some(Self::Go)
        } else if lower.ends_with(".rb") {
            Some(Self::RubyGems)
        } else if lower.ends_with(".java") {
            Some(Self::Maven)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct RepoRef {
    pub org: String,
    pub repo: String,
    pub commit: String,
    pub ecosystem: Option<Ecosystem>,
}

impl RepoRef {
    pub fn lsp_graph(&self) -> String {
        format!("https://repolex.ai/r/{}/{}/lsp/{}", self.org, self.repo, self.commit)
    }

    pub fn dep_graph(&self) -> String {
        format!("https://repolex.ai/r/{}/{}/dep/{}", self.org, self.repo, self.commit)
    }

    #[allow(dead_code)]
    pub fn ast_graph(&self) -> String {
        format!("https://repolex.ai/r/{}/{}/ast/{}", self.org, self.repo, self.commit)
    }

    #[allow(dead_code)]
    pub fn repolex_graph(&self) -> String {
        format!("https://repolex.ai/r/{}/{}/repolex/{}", self.org, self.repo, self.commit)
    }

    #[allow(dead_code)]
    pub fn filetree_graph(&self) -> String {
        format!("https://repolex.ai/r/{}/{}/filetree/{}", self.org, self.repo, self.commit)
    }
}

const BACKBONE: &[(&str, &str, &str, &str, Ecosystem)] = &[
    ("rlex", "repolex-ai", "rlex", "550a8e5a1a7b121bd970eff3e7575acd158f6bb8", Ecosystem::Cargo),
    ("git-lex", "repolex-ai", "git-lex", "d79791e26e44dbd3667f05caa6a97fcd12b3d146", Ecosystem::Cargo),
    ("actix", "actix", "actix-web", "5723cf486522d47aad26390cf5b02e95654ae225", Ecosystem::Cargo),
    ("actix-web", "actix", "actix-web", "5723cf486522d47aad26390cf5b02e95654ae225", Ecosystem::Cargo),
    ("regex", "rust-lang", "regex", "25a15e272b3ae5aee76b525902c2ab91b0d9e12e", Ecosystem::Cargo),
    ("aho-corasick", "BurntSushi", "aho-corasick", "d84a5073d5108fce1774b375105dfdb13fe4e81c", Ecosystem::Cargo),
    ("memchr", "BurntSushi", "memchr", "886ca4ca4820297191c6e9f7b023dc356f31a4d1", Ecosystem::Cargo),
    ("walkdir", "BurntSushi", "walkdir", "4f26be4d450910916ea11533b2efc52b9a6483bc", Ecosystem::Cargo),
    ("axum", "tokio-rs", "axum", "c59208c86fded335cd85e388030ad59347b0e5ae", Ecosystem::Cargo),
    ("bytes", "tokio-rs", "bytes", "417dccdeff249e0c011327de7d92e0d6fbe7cc43", Ecosystem::Cargo),
    ("syn", "dtolnay", "syn", "7bcb37cdb3399977658c8b52d2441d37e42e48f2", Ecosystem::Cargo),
    ("quote", "dtolnay", "quote", "842ffde933fdd76cd1681a288bed136d8b95a97a", Ecosystem::Cargo),
    ("proc-macro2", "dtolnay", "proc-macro2", "58ab776b95a4c2865554badbb6629c50971a9118", Ecosystem::Cargo),
    ("anyhow", "dtolnay", "anyhow", "5c657b32522023a9f7ef883fb08582fd8e656b1a", Ecosystem::Cargo),
    ("serde", "serde-rs", "serde", "7fc3b4c30c94f73a96ebd1553f2b090d928fc3a8", Ecosystem::Cargo),
    ("serde-json", "serde-rs", "json", "4f6dbfac79647d032b0997b5ab73022340c6dab7", Ecosystem::Cargo),
    ("serde_json", "serde-rs", "json", "4f6dbfac79647d032b0997b5ab73022340c6dab7", Ecosystem::Cargo),
    ("socket2", "rust-lang", "socket2", "239dd83a4ced08e514d2c38942aab99791119f0d", Ecosystem::Cargo),
    ("libc", "rust-lang", "libc", "f3417ae86f015b9d0d8789b8fa11e64f9fec0adf", Ecosystem::Cargo),
    ("git2", "rust-lang", "git2-rs", "b863968301f0e889fa04afc590d7e2c9a4100dc3", Ecosystem::Cargo),
    ("git2-rs", "rust-lang", "git2-rs", "b863968301f0e889fa04afc590d7e2c9a4100dc3", Ecosystem::Cargo),
    ("flate2", "rust-lang", "flate2-rs", "93c81772305a102f1ec846bd12713dd7bf1e3f04", Ecosystem::Cargo),
    ("flate2-rs", "rust-lang", "flate2-rs", "93c81772305a102f1ec846bd12713dd7bf1e3f04", Ecosystem::Cargo),
    ("miniz_oxide", "Frommi", "miniz_oxide", "86d92e8d284b0acd451e7b6db6bdef41ae9c9db4", Ecosystem::Cargo),
    ("crc32fast", "srijs", "rust-crc32fast", "a150f65ce810793293d5c9dd815f4510eb6d8e4c", Ecosystem::Cargo),
    ("http", "hyperium", "http", "b9625d83b524f7a8306883484f29a746eefc1bab", Ecosystem::Cargo),
    ("hyper", "hyperium", "hyper", "0d6c7d5469baa09e2fb127ee3758a79b3271a4f0", Ecosystem::Cargo),
    ("reqwest", "seanmonstar", "reqwest", "ad83b63824385a4e5758d263db707549bbe59ba7", Ecosystem::Cargo),
    ("ruby", "ruby", "ruby", "995b59f66677d44767ce9faac6957e5543617ff9", Ecosystem::RubyGems),
    ("pan", "repolex-ai", "pan", "d92f3db1f1b1a997d6290d1208e231d9e455c9d8", Ecosystem::Cargo),
    ("jquery", "jquery", "jquery", "4dec426aa2a6cbabb1b064319ba7c272d594a688", Ecosystem::Npm),
];

fn normalize_pkg_ident(s: &str) -> String {
    s.trim().to_ascii_lowercase().replace('-', "_")
}

/// Resolve a repository query string (e.g. "syn", "regex", "dtolnay/syn")
/// to a RepoRef using exact backbone mappings and exact local cache fallbacks.
pub fn resolve_repo(query: &str, cache_root: Option<&Path>) -> Option<RepoRef> {
    resolve_repo_commit_ecosystem(query, None, None, cache_root)
}

/// Resolve a repository query string and optional commit SHA to a RepoRef.
pub fn resolve_repo_commit(query: &str, commit_opt: Option<&str>, cache_root: Option<&Path>) -> Option<RepoRef> {
    resolve_repo_commit_ecosystem(query, commit_opt, None, cache_root)
}

/// Resolve a repository query string with strict ecosystem isolation and exact matching.
pub fn resolve_repo_for_ecosystem(
    query: &str,
    expected_ecosystem: Option<Ecosystem>,
    cache_root: Option<&Path>,
) -> Option<RepoRef> {
    resolve_repo_commit_ecosystem(query, None, expected_ecosystem, cache_root)
}

pub fn resolve_repo_commit_ecosystem(
    query: &str,
    commit_opt: Option<&str>,
    expected_ecosystem: Option<Ecosystem>,
    cache_root: Option<&Path>,
) -> Option<RepoRef> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return None;
    }
    let clean = q.replace("-rs", "").replace("_rs", "");
    let q_norm = normalize_pkg_ident(&q);
    let clean_norm = normalize_pkg_ident(&clean);

    // 1. Exact match on backbone table (respecting ecosystem if provided)
    for (alias, org, repo, default_commit, eco) in BACKBONE {
        if let Some(exp) = expected_ecosystem
            && *eco != exp
        {
            continue;
        }
        let full = format!("{}/{}", org, repo).to_ascii_lowercase();
        let alias_norm = normalize_pkg_ident(alias);
        let repo_norm = normalize_pkg_ident(repo);
        if q == *alias
            || clean == *alias
            || q == full
            || q_norm == alias_norm
            || clean_norm == alias_norm
            || (!q.contains('/') && (q_norm == repo_norm || clean_norm == repo_norm))
        {
            let commit = commit_opt.unwrap_or(default_commit).to_string();
            return Some(RepoRef {
                org: org.to_string(),
                repo: repo.to_string(),
                commit,
                ecosystem: Some(*eco),
            });
        }
    }

    // 2. Exact match in local cache directory (NO substring .contains() matching!)
    if let Some(cache_dir) = cache_root
        && cache_dir.exists()
        && let Ok(org_entries) = std::fs::read_dir(cache_dir)
    {
        for org_entry in org_entries.flatten() {
            let org_name = org_entry.file_name().to_string_lossy().to_string();
            if let Ok(repo_entries) = std::fs::read_dir(org_entry.path()) {
                for repo_entry in repo_entries.flatten() {
                    let repo_name = repo_entry.file_name().to_string_lossy().to_string();
                    let full_name = format!("{}/{}", org_name, repo_name).to_ascii_lowercase();
                    let repo_lower = repo_name.to_ascii_lowercase();
                    let repo_norm = normalize_pkg_ident(&repo_lower);

                    let is_match = if q.contains('/') {
                        full_name == q
                    } else {
                        repo_lower == q
                            || repo_lower == clean
                            || repo_norm == q_norm
                            || repo_norm == clean_norm
                    };

                    if is_match && let Ok(commits) = std::fs::read_dir(repo_entry.path()) {
                        let mut commit_dirs = Vec::new();
                        for c in commits.flatten() {
                            if c.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                                let c_sha = c.file_name().to_string_lossy().to_string();
                                commit_dirs.push((c_sha, c.path()));
                            }
                        }

                        // If user specified a commit, find matching prefix
                        if let Some(req_sha) = commit_opt {
                            for (c_sha, _) in &commit_dirs {
                                if c_sha.starts_with(req_sha) || req_sha.starts_with(c_sha) {
                                    return Some(RepoRef {
                                        org: org_name,
                                        repo: repo_name,
                                        commit: c_sha.clone(),
                                        ecosystem: None,
                                    });
                                }
                            }
                            return Some(RepoRef {
                                org: org_name,
                                repo: repo_name,
                                commit: req_sha.to_string(),
                                ecosystem: None,
                            });
                        }

                        // Prioritize commit with lsp or dep graphs
                        for (c_sha, c_path) in &commit_dirs {
                            if c_path.join("lsp").exists() || c_path.join("dep").exists() {
                                return Some(RepoRef {
                                    org: org_name,
                                    repo: repo_name,
                                    commit: c_sha.clone(),
                                    ecosystem: None,
                                });
                            }
                        }

                        if let Some((c_sha, _)) = commit_dirs.first() {
                            return Some(RepoRef {
                                org: org_name,
                                repo: repo_name,
                                commit: c_sha.clone(),
                                ecosystem: None,
                            });
                        }
                    }
                }
            }
        }
    }

    None
}

/// Detect the package ecosystem of a repository using its known metadata,
/// its `dep` graph (`rx:packageEcosystem`), or its `lsp` graph file extensions.
pub fn detect_ecosystem(client: &Client, repo: &RepoRef) -> Option<Ecosystem> {
    if let Some(eco) = repo.ecosystem {
        return Some(eco);
    }

    // 1. Check dep graph for rx:packageEcosystem
    let q_dep = format!(
        r#"
PREFIX rx: <https://repolex.ai/ontology/repolex/>
SELECT ?eco WHERE {{
  GRAPH <{}> {{
    ?s rx:packageEcosystem ?eco .
  }}
}}
LIMIT 1
"#,
        repo.dep_graph()
    );
    if let Ok(res) = client.query(&q_dep)
        && let Some(val) = res.rows.first().and_then(|r| r.first())
        && let Some(eco) = Ecosystem::from_str_opt(val)
    {
        return Some(eco);
    }

    // 2. Fallback: infer from lsp graph resolutionSourceFile extension
    let q_lsp = format!(
        r#"
PREFIX lx: <https://repolex.ai/ontology/repolex/lsp-extension/>
SELECT ?f WHERE {{
  GRAPH <{}> {{
    ?e lx:resolutionSourceFile ?f .
  }}
}}
LIMIT 10
"#,
        repo.lsp_graph()
    );
    if let Ok(res) = client.query(&q_lsp) {
        for row in &res.rows {
            if let Some(f) = row.first()
                && let Some(eco) = Ecosystem::from_file_path(f)
            {
                return Some(eco);
            }
        }
    }

    None
}

/// Extract `org/repo` from a repolex callTarget IRI, e.g.
/// `https://repolex.ai/r/BurntSushi/aho-corasick/commit/...` or `r:BurntSushi/aho-corasick/commit/...`.
pub fn extract_org_repo_from_call_target(iri: &str) -> Option<String> {
    let rest = iri
        .strip_prefix("https://repolex.ai/r/")
        .or_else(|| iri.strip_prefix("r:"))?;
    let mut parts = rest.split('/');
    let org = parts.next()?;
    let repo = parts.next()?;
    if org.is_empty() || repo.is_empty() {
        return None;
    }
    Some(format!("{}/{}", org, repo))
}

/// Resolve a dependency package from `parent` with strict ecosystem isolation:
/// 1. If `sample_call_target` encodes `org/repo`, resolve that exact `org/repo`.
/// 2. Check `parent`'s `dep` graph for an exact `rx:packageName` match -> `githubOrg/githubRepo`.
/// 3. Fall back to exact package match in backbone/cache scoped to `parent_eco`.
/// 4. Verify the resolved candidate does not belong to a different ecosystem.
pub fn resolve_dep_repo(
    client: &Client,
    parent: &RepoRef,
    parent_eco: Option<Ecosystem>,
    target_pkg: &str,
    sample_call_target: Option<&str>,
    cache_root: Option<&Path>,
) -> Option<RepoRef> {
    let mut candidate: Option<RepoRef> = None;

    // 1. Direct org/repo from callTarget IRI
    if let Some(target_iri) = sample_call_target
        && let Some(org_repo) = extract_org_repo_from_call_target(target_iri)
    {
        candidate = resolve_repo_for_ecosystem(&org_repo, parent_eco, cache_root)
            .or_else(|| resolve_repo(&org_repo, cache_root));
    }

    // 2. Lookup exact packageName in parent's dep graph
    if candidate.is_none() {
        let escaped_pkg = target_pkg.replace('\\', "\\\\").replace('"', "\\\"");
        let q_dep = format!(
            r#"
PREFIX rx: <https://repolex.ai/ontology/repolex/>
SELECT ?org ?repo ?eco WHERE {{
  GRAPH <{}> {{
    ?s rx:packageName ?pkg ;
       rx:githubOrg ?org ;
       rx:githubRepo ?repo .
    OPTIONAL {{ ?s rx:packageEcosystem ?eco . }}
    FILTER(LCASE(STR(?pkg)) = LCASE("{}"))
  }}
}}
LIMIT 1
"#,
            parent.dep_graph(),
            escaped_pkg
        );
        if let Ok(res) = client.query(&q_dep)
            && let Some(row) = res.rows.first()
            && let (Some(org), Some(repo)) = (row.first(), row.get(1))
        {
            let dep_eco = row.get(2).and_then(|e| Ecosystem::from_str_opt(e)).or(parent_eco);
            let org_repo = format!("{}/{}", org, repo);
            candidate = resolve_repo_for_ecosystem(&org_repo, dep_eco, cache_root)
                .or_else(|| resolve_repo(&org_repo, cache_root));
        }
    }

    // 3. Fallback: exact match in backbone/cache scoped to parent_eco
    if candidate.is_none() {
        candidate = resolve_repo_for_ecosystem(target_pkg, parent_eco, cache_root);
    }

    // 4. Strict ecosystem gate: reject cross-ecosystem hops
    let mut resolved = candidate?;
    if let Some(pe) = parent_eco {
        let cand_eco = detect_ecosystem(client, &resolved);
        if let Some(ce) = cand_eco {
            if ce != pe {
                return None;
            }
            resolved.ecosystem = Some(ce);
        }
    }

    Some(resolved)
}

/// Get all backbone repositories
pub fn get_backbone() -> Vec<RepoRef> {
    let mut seen = std::collections::HashSet::new();
    let mut repos = Vec::new();
    for (_, org, repo, commit, eco) in BACKBONE {
        let key = format!("{}/{}", org, repo);
        if seen.insert(key) {
            repos.push(RepoRef {
                org: org.to_string(),
                repo: repo.to_string(),
                commit: commit.to_string(),
                ecosystem: Some(*eco),
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
        assert_eq!(repo.ecosystem, Some(Ecosystem::Cargo));
        assert!(repo.lsp_graph().contains("rust-lang/regex/lsp/"));
    }

    #[test]
    fn test_resolve_with_owner() {
        let repo = resolve_repo("dtolnay/syn", None).expect("dtolnay/syn should resolve");
        assert_eq!(repo.org, "dtolnay");
        assert_eq!(repo.repo, "syn");
        assert_eq!(repo.ecosystem, Some(Ecosystem::Cargo));
    }

    #[test]
    fn test_no_substring_bleed_in_backbone_or_cache() {
        // Substring of "regex" or "log" must NOT match unrelated repos
        assert!(resolve_repo("reg", None).is_none());
        assert!(resolve_repo("log", None).is_none());

        // Simulate cache containing concurrent-log-handler
        let tmp = std::env::temp_dir().join("rlex_test_registry_no_bleed");
        let _ = std::fs::remove_dir_all(&tmp);
        let commit_dir = tmp.join("Preston-Landers/concurrent-log-handler/deadbeef/lsp");
        std::fs::create_dir_all(&commit_dir).unwrap();

        // Bare "log" must NOT match "concurrent-log-handler"
        assert!(resolve_repo("log", Some(&tmp)).is_none());
        // Exact name still resolves
        let exact = resolve_repo("concurrent-log-handler", Some(&tmp)).expect("exact cache match");
        assert_eq!(exact.repo, "concurrent-log-handler");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_ecosystem_isolation() {
        assert!(resolve_repo_for_ecosystem("regex", Some(Ecosystem::Cargo), None).is_some());
        assert!(resolve_repo_for_ecosystem("regex", Some(Ecosystem::Pypi), None).is_none());
        assert!(resolve_repo_for_ecosystem("jquery", Some(Ecosystem::Npm), None).is_some());
        assert!(resolve_repo_for_ecosystem("jquery", Some(Ecosystem::Cargo), None).is_none());
    }

    #[test]
    fn test_extract_org_repo_from_call_target() {
        assert_eq!(
            extract_org_repo_from_call_target(
                "https://repolex.ai/r/BurntSushi/aho-corasick/commit/8eb6f5a3/src/lib.rs#foo"
            ),
            Some("BurntSushi/aho-corasick".to_string())
        );
        assert_eq!(
            extract_org_repo_from_call_target("r:rust-lang/log/commit/8034743/src/lib.rs#info"),
            Some("rust-lang/log".to_string())
        );
    }
}
