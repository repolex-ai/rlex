#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScopeFilter {
    pub include_tests: bool,
    pub include_benches: bool,
}

impl ScopeFilter {
    #[allow(dead_code)]
    pub fn prod_only() -> Self {
        Self {
            include_tests: false,
            include_benches: false,
        }
    }

    pub fn label(&self) -> &'static str {
        match (self.include_tests, self.include_benches) {
            (false, false) => "prod-only",
            (true, false) => "prod+tests",
            (false, true) => "prod+benches",
            (true, true) => "all (prod+tests+benches)",
        }
    }

    /// Generate SPARQL FILTER clauses for a bound path or target-IRI variable (e.g. "?srcFile").
    pub fn sparql_filter(&self, var: &str) -> String {
        let mut clauses = Vec::new();
        if !self.include_tests {
            clauses.push(format!(
                r#"FILTER(!REGEX(STR({var}), "(^|/)([^/]*-tests?|tests?|__tests__|spec|specs|testdata|fixtures)(/|$)|(_test\\.|\\.test\\.|\\.spec\\.|(^|/)test_[^/]+$)", "i"))"#
            ));
        }
        if !self.include_benches {
            clauses.push(format!(
                r#"FILTER(!REGEX(STR({var}), "(^|/)(benches|bench|benchmarks|benchmark)(/|$)|(_bench\\.|\\.bench\\.|(^|/)bench_[^/]+$)", "i"))"#
            ));
        }
        clauses.join("\n    ")
    }

    /// Generate SPARQL FILTER clauses for an OPTIONAL path variable (passes if unbound).
    pub fn sparql_filter_optional(&self, var: &str) -> String {
        let mut clauses = Vec::new();
        if !self.include_tests {
            clauses.push(format!(
                r#"FILTER(!BOUND({var}) || !REGEX(STR({var}), "(^|/)([^/]*-tests?|tests?|__tests__|spec|specs|testdata|fixtures)(/|$)|(_test\\.|\\.test\\.|\\.spec\\.|(^|/)test_[^/]+$)", "i"))"#
            ));
        }
        if !self.include_benches {
            clauses.push(format!(
                r#"FILTER(!BOUND({var}) || !REGEX(STR({var}), "(^|/)(benches|bench|benchmarks|benchmark)(/|$)|(_bench\\.|\\.bench\\.|(^|/)bench_[^/]+$)", "i"))"#
            ));
        }
        clauses.join("\n    ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_filter_labels() {
        assert_eq!(ScopeFilter::prod_only().label(), "prod-only");
        assert_eq!(
            ScopeFilter { include_tests: true, include_benches: false }.label(),
            "prod+tests"
        );
        assert_eq!(
            ScopeFilter { include_tests: false, include_benches: true }.label(),
            "prod+benches"
        );
        assert_eq!(
            ScopeFilter { include_tests: true, include_benches: true }.label(),
            "all (prod+tests+benches)"
        );
    }

    #[test]
    fn test_scope_filter_sparql_generation() {
        let prod = ScopeFilter::prod_only();
        let f = prod.sparql_filter("?src");
        assert!(f.contains("tests?"));
        assert!(f.contains("benches"));

        let all = ScopeFilter { include_tests: true, include_benches: true };
        assert!(all.sparql_filter("?src").is_empty());
        assert!(all.sparql_filter_optional("?src").is_empty());
    }
}
