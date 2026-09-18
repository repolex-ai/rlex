use crate::registry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossCommitEquivalence {
    pub package: &'static str,
    pub org: &'static str,
    pub repo: &'static str,
    pub lockfile_sha: &'static str,
    pub canonical_sha: &'static str,
    pub release_tag: Option<&'static str>,
}

/// Known cross-commit equivalences linking lockfile dependency commit SHAs
/// to the canonical release tag commit SHAs ingested in Repolex knowledge graphs.
pub const KNOWN_EQUIVALENCES: &[CrossCommitEquivalence] = &[
    CrossCommitEquivalence {
        package: "syn",
        org: "dtolnay",
        repo: "syn",
        lockfile_sha: "0e4bc64fe1e07a574b6f3133927a27991fd40c2e",
        canonical_sha: "7bcb37cdb3399977658c8b52d2441d37e42e48f2",
        release_tag: Some("2.0.117"),
    },
    CrossCommitEquivalence {
        package: "regex",
        org: "rust-lang",
        repo: "regex",
        lockfile_sha: "2b527599eb9eea0dcc288c704584f242f26a5c61",
        canonical_sha: "25a15e272b3ae5aee76b525902c2ab91b0d9e12e",
        release_tag: Some("rure-0.2.3"),
    },
    CrossCommitEquivalence {
        package: "quote",
        org: "dtolnay",
        repo: "quote",
        lockfile_sha: "b01743f24cb5b19f96a3eac6bce0e7aee10f6199",
        canonical_sha: "842ffde933fdd76cd1681a288bed136d8b95a97a",
        release_tag: Some("1.0.38"),
    },
    CrossCommitEquivalence {
        package: "proc-macro2",
        org: "dtolnay",
        repo: "proc-macro2",
        lockfile_sha: "da51f8d005cc5d8299c1872fad9bbe63b07c31c7",
        canonical_sha: "58ab776b95a4c2865554badbb6629c50971a9118",
        release_tag: Some("1.0.92"),
    },
    CrossCommitEquivalence {
        package: "aho-corasick",
        org: "BurntSushi",
        repo: "aho-corasick",
        lockfile_sha: "8eb6f5a3e8348ad80f16d2174c3d387064a1cc2e",
        canonical_sha: "d84a5073d5108fce1774b375105dfdb13fe4e81c",
        release_tag: Some("1.1.3"),
    },
    CrossCommitEquivalence {
        package: "memchr",
        org: "BurntSushi",
        repo: "memchr",
        lockfile_sha: "0ba6735559ae9c1cb7e6470a97834a95cc06fbbb",
        canonical_sha: "886ca4ca4820297191c6e9f7b023dc356f31a4d1",
        release_tag: Some("2.7.4"),
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCallTarget {
    pub org: String,
    pub repo: String,
    pub commit_sha: String,
    pub rel_path: String,
    pub fragment: Option<String>,
}

/// Parse a callTarget IRI into its constituent components:
/// format: https://repolex.ai/r/{org}/{repo}/commit/{sha}/{rel_path}#{fragment}
pub fn parse_call_target(iri: &str) -> Option<ParsedCallTarget> {
    let clean = iri.trim();
    let without_prefix = clean
        .strip_prefix("https://repolex.ai/r/")
        .or_else(|| clean.strip_prefix("r:"))?;

    let (path_part, fragment) = match without_prefix.split_once('#') {
        Some((p, f)) => (p, Some(f.to_string())),
        None => (without_prefix, None),
    };

    let segments: Vec<&str> = path_part.splitn(5, '/').collect();
    if segments.len() < 5 || segments[2] != "commit" {
        return None;
    }

    Some(ParsedCallTarget {
        org: segments[0].to_string(),
        repo: segments[1].to_string(),
        commit_sha: segments[3].to_string(),
        rel_path: segments[4].to_string(),
        fragment,
    })
}

/// Look up the canonical release commit SHA for a given repo or package name and lockfile SHA.
pub fn resolve_canonical_sha(package_or_repo: &str, lockfile_sha: &str) -> Option<String> {
    let clean_pkg = package_or_repo.trim().to_lowercase();

    // 1. Exact match in known equivalences table
    for eq in KNOWN_EQUIVALENCES {
        if eq.lockfile_sha == lockfile_sha
            && (clean_pkg.is_empty()
                || clean_pkg == eq.package
                || clean_pkg == eq.repo
                || clean_pkg == format!("{}/{}", eq.org, eq.repo))
        {
            return Some(eq.canonical_sha.to_string());
        }
    }

    // 2. Fall back to registry backbone canonical commit for the repo
    if let Some(repo_ref) = registry::resolve_repo(&clean_pkg, None) {
        return Some(repo_ref.commit);
    }

    None
}

/// Canonicalize a callTarget IRI by mapping any dependency lockfile commit SHA
/// to its canonical release commit SHA in the Repolex knowledge graph.
///
/// Returns (canonical_iri, Option<original_lockfile_sha>).
pub fn canonicalize_call_target(iri: &str) -> (String, Option<String>) {
    let parsed = match parse_call_target(iri) {
        Some(p) => p,
        None => return (iri.to_string(), None),
    };

    // Check if the commit SHA is in the equivalence table
    if let Some(canonical_sha) = resolve_canonical_sha(&parsed.repo, &parsed.commit_sha)
        .filter(|sha| sha != &parsed.commit_sha)
    {
        let fragment_part = parsed
            .fragment
            .as_deref()
            .map(|f| format!("#{}", f))
            .unwrap_or_default();

        let prefix = if iri.trim().starts_with("r:") {
            "r:"
        } else {
            "https://repolex.ai/r/"
        };

        let canonical_iri = format!(
            "{}{}/{}/commit/{}/{}{}",
            prefix, parsed.org, parsed.repo, canonical_sha, parsed.rel_path, fragment_part
        );
        return (canonical_iri, Some(parsed.commit_sha));
    }

    (iri.to_string(), None)
}

/// Generate SPARQL query snippets for bridging cross-commit call edges:
/// Extracts the relative file path from callTarget to join with the callee's resolutionSourceFile.
#[allow(dead_code)]
pub fn generate_file_bridge_bind(call_target_var: &str, target_file_var: &str) -> String {
    format!(
        "BIND(REPLACE(REPLACE(STR({}), '^.*commit/[^/]+/', ''), '#.*$', '') AS {})",
        call_target_var, target_file_var
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_call_target() {
        let iri = "https://repolex.ai/r/dtolnay/syn/commit/0e4bc64fe1e07a574b6f3133927a27991fd40c2e/src/punctuated.rs#identifier_71_11_71_14";
        let parsed = parse_call_target(iri).expect("must parse");
        assert_eq!(parsed.org, "dtolnay");
        assert_eq!(parsed.repo, "syn");
        assert_eq!(parsed.commit_sha, "0e4bc64fe1e07a574b6f3133927a27991fd40c2e");
        assert_eq!(parsed.rel_path, "src/punctuated.rs");
        assert_eq!(parsed.fragment.as_deref(), Some("identifier_71_11_71_14"));
    }

    #[test]
    fn test_canonicalize_syn() {
        let lockfile_iri = "https://repolex.ai/r/dtolnay/syn/commit/0e4bc64fe1e07a574b6f3133927a27991fd40c2e/src/punctuated.rs#identifier_71_11_71_14";
        let (canonical, original_sha) = canonicalize_call_target(lockfile_iri);
        assert_eq!(original_sha.as_deref(), Some("0e4bc64fe1e07a574b6f3133927a27991fd40c2e"));
        assert_eq!(
            canonical,
            "https://repolex.ai/r/dtolnay/syn/commit/7bcb37cdb3399977658c8b52d2441d37e42e48f2/src/punctuated.rs#identifier_71_11_71_14"
        );

        // Also test r: compact prefix
        let compact_iri = "r:dtolnay/syn/commit/0e4bc64fe1e07a574b6f3133927a27991fd40c2e/src/punctuated.rs#identifier_71_11_71_14";
        let (canonical_compact, original_compact_sha) = canonicalize_call_target(compact_iri);
        assert_eq!(original_compact_sha.as_deref(), Some("0e4bc64fe1e07a574b6f3133927a27991fd40c2e"));
        assert_eq!(
            canonical_compact,
            "r:dtolnay/syn/commit/7bcb37cdb3399977658c8b52d2441d37e42e48f2/src/punctuated.rs#identifier_71_11_71_14"
        );
    }

    #[test]
    fn test_canonicalize_regex() {
        let lockfile_iri = "https://repolex.ai/r/rust-lang/regex/commit/2b527599eb9eea0dcc288c704584f242f26a5c61/src/regex/string.rs#identifier_228";
        let (canonical, original_sha) = canonicalize_call_target(lockfile_iri);
        assert_eq!(original_sha.as_deref(), Some("2b527599eb9eea0dcc288c704584f242f26a5c61"));
        assert_eq!(
            canonical,
            "https://repolex.ai/r/rust-lang/regex/commit/25a15e272b3ae5aee76b525902c2ab91b0d9e12e/src/regex/string.rs#identifier_228"
        );
    }

    #[test]
    fn test_file_bridge_bind() {
        let snippet = generate_file_bridge_bind("?target", "?file");
        assert!(snippet.contains("REPLACE"));
        assert!(snippet.contains("?target"));
        assert!(snippet.contains("?file"));
    }
}
