/// SPARQL Query Linter & Unindexed Scan Guard
///
/// Guards against full-store sequential scans (e.g. the "CONTAINS trap") across
/// Repolex's 118M+ quad store by checking query structure before execution.

#[derive(Debug, Clone)]
pub enum HazardKind {
    /// FILTER(CONTAINS(...)) or FILTER(REGEX(...)) without an exact named GRAPH <iri>
    UnanchoredStringFilter,
    /// Open triple pattern ?s ?p ?o without indexed predicates or graph boundaries
    UnboundedWildcardScan,
}

#[derive(Debug, Clone)]
pub struct ScanHazard {
    #[allow(dead_code)]
    pub kind: HazardKind,
    pub message: String,
    pub suggestion: String,
}

/// Analyze a SPARQL query string for hazards that force sequential full-store scans.
pub fn lint_query(sparql: &str) -> Vec<ScanHazard> {
    let mut hazards = Vec::new();
    let upper = sparql.to_uppercase();

    // Check if query is anchored to an exact named graph: GRAPH <iri>
    let has_bound_graph = check_has_bound_graph(sparql);

    // 1. Check for unanchored string filter (CONTAINS, REGEX, STRSTARTS, STRENDS)
    let has_string_filter = upper.contains("FILTER")
        && (upper.contains("CONTAINS")
            || upper.contains("REGEX")
            || upper.contains("STRSTARTS")
            || upper.contains("STRENDS"));

    if has_string_filter && !has_bound_graph {
        hazards.push(ScanHazard {
            kind: HazardKind::UnanchoredStringFilter,
            message: "Unanchored string filter detected (The CONTAINS / REGEX trap)".into(),
            suggestion: "Anchor the pattern to an exact named graph: GRAPH <https://repolex.ai/r/org/repo/graph_type/commit> { ... } or bind the predicate (e.g. lx:externalPackage) to avoid a full 118M+ quad scan (30s+ vs 1.15ms).".into(),
        });
    }

    // 2. Check for unbounded ?s ?p ?o scans
    let has_unbounded_wildcard = check_unbounded_wildcard(sparql);
    if has_unbounded_wildcard && !has_bound_graph {
        hazards.push(ScanHazard {
            kind: HazardKind::UnboundedWildcardScan,
            message: "Unbounded wildcard triple scan (?s ?p ?o) without graph boundaries".into(),
            suggestion: "Wrap patterns inside a specific GRAPH <iri> or bind the predicate to an indexed term (e.g. lx:callTarget, rdf:type) to leverage RocksDB index blocks.".into(),
        });
    }

    hazards
}

fn check_has_bound_graph(sparql: &str) -> bool {
    let lines = sparql.lines();
    for line in lines {
        let trimmed = line.trim();
        // Check for GRAPH <iri> or FROM <iri>
        if trimmed.to_uppercase().starts_with("GRAPH <")
            || trimmed.to_uppercase().starts_with("FROM <")
            || trimmed.to_uppercase().starts_with("FROM NAMED <")
        {
            return true;
        }
        // In-line GRAPH <...>
        if let Some(idx) = trimmed.to_uppercase().find("GRAPH <") {
            let rest = &trimmed[idx..];
            if rest.contains('>') {
                return true;
            }
        }
    }
    false
}

fn check_unbounded_wildcard(sparql: &str) -> bool {
    let re = regex::Regex::new(r#"\?\w+\s+\?\w+\s+\?\w+\s*\."#).ok();
    if let Some(ref r) = re {
        r.is_match(sparql)
    } else {
        false
    }
}

/// Print formatted warning banner to stderr
pub fn print_hazards(hazards: &[ScanHazard]) {
    if hazards.is_empty() {
        return;
    }

    eprintln!("\n┌──────────────────────────────────────────────────────────────────────────────┐");
    eprintln!("│ ⚠  REPOLEX SCAN GUARD WARNING: Potential Full-Store Scan Detected            │");
    eprintln!("├──────────────────────────────────────────────────────────────────────────────┤");
    for (i, h) in hazards.iter().enumerate() {
        eprintln!("│ Issue #{}: {:<67} │", i + 1, h.message);
        eprintln!("│ Tip:     {:<67} │", wrap_text(&h.suggestion, 67));
    }
    eprintln!("│ Note: Running unanchored scans across 118,647,867 quads may take 30+ seconds.│");
    eprintln!("│ Use `rlex moreinfo scan-guard` or `rlex warmup` for guidance.                │");
    eprintln!("└──────────────────────────────────────────────────────────────────────────────┘\n");
}

fn wrap_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}...", &text[..max_len - 3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catches_unanchored_contains() {
        let q = "SELECT * WHERE { ?s ?p ?o . FILTER(CONTAINS(STR(?s), \"regex\")) }";
        let hazards = lint_query(q);
        assert!(!hazards.is_empty());
        assert!(hazards.iter().any(|h| matches!(h.kind, HazardKind::UnanchoredStringFilter)));
    }

    #[test]
    fn test_catches_unbounded_wildcard() {
        let q = "SELECT * WHERE { ?s ?p ?o . }";
        let hazards = lint_query(q);
        assert!(!hazards.is_empty());
        assert!(hazards.iter().any(|h| matches!(h.kind, HazardKind::UnboundedWildcardScan)));
    }

    #[test]
    fn test_allows_anchored_graph() {
        let q = "SELECT * WHERE { GRAPH <https://repolex.ai/r/dtolnay/syn/lsp/123> { ?s ?p ?o } }";
        let hazards = lint_query(q);
        assert!(hazards.is_empty());
    }
}
