use anyhow::Result;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct TopicDoc {
    pub topic: &'static str,
    pub title: &'static str,
    pub tags: &'static [&'static str],
    pub summary: &'static str,
    pub sparql_recipe: Option<&'static str>,
    pub content: &'static str,
}

const TOPICS: &[TopicDoc] = &[
    TopicDoc {
        topic: "recipes",
        title: "SPARQL Query Recipes Cheat Sheet",
        tags: &["recipes", "sparql", "queries", "quickstart"],
        summary: "Index of tested, high-performance SPARQL recipes for code intelligence across 118M+ quads.",
        sparql_recipe: None,
        content: r#"# Repolex SPARQL Query Recipes Cheat Sheet

Repolex stores over 118,647,867 quads indexing AST syntax blobs, semantic call sites,
LSP cross-repository call edges, and package dependencies.

### Top Commands & Equivalent SPARQL Recipes

1. **Multi-Hop Call Traversal**:
   - CLI: `rlex calls --from regex --to memchr`
   - See topic: `rlex moreinfo closures`

2. **Transitive Call Closures**:
   - CLI: `rlex closure --from actix-web --depth 3`
   - Discovers all reachable transitive dependencies with call counts and source files.

3. **Diamond Dependency Convergence**:
   - CLI: `rlex diamond --from syn`
   - Identifies downstream crates reached via multiple distinct branches (e.g. `syn` ➔ `quote` ➔ `proc-macro2` and `syn` ➔ `proc-macro2`).
   - See topic: `rlex moreinfo diamond`

4. **Cyclic Dependency & Recursion Detection**:
   - CLI: `rlex cycles --repo actix-web`
   - Detects mutual recursion or cyclic dependencies.
   - See topic: `rlex moreinfo cycles`

5. **Query Optimization & Scan Guard**:
   - CLI: `rlex warmup`
   - Detects and prevents the CONTAINS trap (avoiding 30s unindexed scans).
   - See topic: `rlex moreinfo scan-guard`

6. **Ontology Prefixes**:
   - See topic: `rlex moreinfo prefixes`
"#,
    },
    TopicDoc {
        topic: "closures",
        title: "Transitive Call Closures across Repository Boundaries",
        tags: &["closure", "transitive", "call-graph", "reachability"],
        summary: "Discover deep downstream call graphs across package boundaries using SPARQL property paths and bounded joins.",
        sparql_recipe: Some(r#"PREFIX lx: <https://repolex.ai/ontology/repolex/lsp-extension/>
SELECT ?callerPkg ?targetPkg (COUNT(?e) as ?callCount) (SAMPLE(?src) as ?sampleSourceFile)
WHERE {
  GRAPH <https://repolex.ai/r/rust-lang/regex/lsp/25a15e272b3ae5aee76b525902c2ab91b0d9e12e> {
    ?e lx:resolutionSourceFile ?src ;
       lx:externalPackage ?targetPkg .
  }
}
GROUP BY ?callerPkg ?targetPkg
ORDER BY DESC(?callCount)"#),
        content: r#"# Transitive Call Closures

A transitive call closure calculates all downstream dependencies and functions reachable from
a given source repository, traversing across repository boundaries.

### Why This Matters

Standard package managers (like `cargo tree`) show compile-time crate dependencies, but NOT
whether any code in your binary actually invokes functions in those transitive dependencies.
Repolex provides **call-level reachability** — proving whether execution paths exist.

### 2-Hop Bounded Closure Recipe:

```sparql
PREFIX lx: <https://repolex.ai/ontology/repolex/lsp-extension/>
SELECT ?regexFile ?ahoFile ?memchrTarget
WHERE {
  GRAPH <https://repolex.ai/r/rust-lang/regex/lsp/25a15e272b3ae5aee76b525902c2ab91b0d9e12e> {
    ?e1 lx:resolutionSourceFile ?regexFile ;
        lx:externalPackage "aho-corasick" .
  }
  GRAPH <https://repolex.ai/r/BurntSushi/aho-corasick/lsp/d84a5073d5108fce1774b375105dfdb13fe4e81c> {
    ?e2 lx:resolutionSourceFile ?ahoFile ;
        lx:externalPackage "memchr" ;
        lx:callTarget ?memchrTarget .
  }
}
LIMIT 20
```

### CLI Command:
`rlex closure --from regex --depth 3`
`rlex closure --from actix-web --to memchr`
"#,
    },
    TopicDoc {
        topic: "diamond",
        title: "Diamond Dependency & Call Graph Convergence",
        tags: &["diamond", "convergence", "dependencies", "architecture"],
        summary: "Identify converging dependencies where two distinct paths meet at the same downstream package.",
        sparql_recipe: Some(r#"PREFIX lx: <https://repolex.ai/ontology/repolex/lsp-extension/>
SELECT DISTINCT ?synFile ?quoteFile ?pmTargetDirect ?pmTargetTransitive
WHERE {
  GRAPH <https://repolex.ai/r/dtolnay/syn/lsp/7bcb37cdb3399977658c8b52d2441d37e42e48f2> {
    ?e1 lx:resolutionSourceFile ?synFile ;
        lx:externalPackage "quote" .
    ?e2 lx:externalPackage "proc-macro2" ;
        lx:callTarget ?pmTargetDirect .
  }
  GRAPH <https://repolex.ai/r/dtolnay/quote/lsp/842ffde933fdd76cd1681a288bed136d8b95a97a> {
    ?e3 lx:resolutionSourceFile ?quoteFile ;
        lx:externalPackage "proc-macro2" ;
        lx:callTarget ?pmTargetTransitive .
  }
}
LIMIT 10"#),
        content: r#"# Diamond Dependency Convergence

A diamond dependency pattern occurs when a root component invokes two or more distinct
intermediate components, which then both converge on the same downstream dependency:

```
         Root (e.g. syn)
          /           \
         /             \
   Branch A (quote)   Direct / Branch B
         \             /
          \           /
     Converged Target (proc-macro2)
```

### Why Diamonds Matter

1. **Version Skew Hazards**: If Branch A depends on v1.0 and Branch B depends on v2.0 of the target.
2. **Coupling Density**: Diamonds reveal the foundational architectural hubs of an ecosystem.
3. **Dead-Code Elimination Impact**: Modifying the converged target affects multiple ingress paths simultaneously.

### Live Receipts from Repolex Store:
- `syn` ➔ `quote` (435 calls) ➔ `proc-macro2` (72 calls) AND `syn` ➔ `proc-macro2` directly (552 calls)!
- `regex` ➔ `aho-corasick` (21 calls) ➔ `memchr` (10 calls) AND `regex` ➔ `memchr` directly (16 calls)!

### CLI Command:
`rlex diamond --from syn`
`rlex diamond --from regex`
"#,
    },
    TopicDoc {
        topic: "cycles",
        title: "Cyclic Dependency & Mutual Recursion Detection",
        tags: &["cycles", "recursion", "circular", "architecture"],
        summary: "Detect recursive call cycles between modules or circular dependencies across crates.",
        sparql_recipe: Some(r#"PREFIX lx: <https://repolex.ai/ontology/repolex/lsp-extension/>
SELECT DISTINCT ?srcA ?srcB
WHERE {
  GRAPH <https://repolex.ai/r/org/repo/lsp/commit_sha> {
    ?e1 lx:resolutionSourceFile ?srcA ;
        lx:callTargetFile ?srcB .
    ?e2 lx:resolutionSourceFile ?srcB ;
        lx:callTargetFile ?srcA .
    FILTER(STR(?srcA) < STR(?srcB))
  }
}
LIMIT 20"#),
        content: r#"# Cyclic Dependency & Mutual Recursion Detection

Cycles in software architecture represent tight coupling, making refactoring and modularization
difficult. In compiled languages, cross-crate cycles are forbidden by compilers, but internal
cross-file cycles or mutual function recursion can still occur.

### Types of Cycles

1. **Internal Cross-File Cycles**:
   Module A calls Module B, and Module B calls Module A.
2. **Mutual Function Recursion**:
   `funcA()` calls `funcB()`, and `funcB()` calls `funcA()`.
3. **Ecosystem Circularity**:
   Package A lists Package B as dependency, and Package B depends on Package A.

### CLI Command:
`rlex cycles --repo actix-web`
`rlex cycles` (scans entire ecosystem backbone)
"#,
    },
    TopicDoc {
        topic: "scan-guard",
        title: "The CONTAINS Trap & Query Engine Optimizations",
        tags: &["scan-guard", "optimization", "contains-trap", "rocksdb", "cache"],
        summary: "Why unanchored FILTER(CONTAINS(...)) scans take 30+ seconds and how named-graph anchoring runs in 1.15ms.",
        sparql_recipe: Some(r#"# BAD: Sequential full-store scan (30+ seconds across 118M quads)
SELECT * WHERE { ?s ?p ?o . FILTER(CONTAINS(STR(?s), "regex")) }

# GOOD: Exact named graph lookup (1.15 ms across 118M quads)
PREFIX lx: <https://repolex.ai/ontology/repolex/lsp-extension/>
SELECT ?file ?target WHERE {
  GRAPH <https://repolex.ai/r/rust-lang/regex/lsp/25a15e272b3ae5aee76b525902c2ab91b0d9e12e> {
    ?e lx:resolutionSourceFile ?file ;
       lx:callTarget ?target .
  }
}"#),
        content: r#"# The CONTAINS Trap & Query Engine Optimizations

Oxigraph indexes 118,647,867 quads using RocksDB multi-column indexes:
- `SPO`, `POS`, `OSP` for default graph
- `GSPO`, `GPOS`, `GOSP` for quad/named-graph queries

### The CONTAINS Trap

When you write:
```sparql
SELECT * WHERE {
  ?s ?p ?o .
  FILTER(CONTAINS(STR(?s), "regex"))
}
```
Oxigraph has no index on substring evaluation. It must perform a **full sequential scan** of all
118 million records, taking 30 to 45 seconds!

### The Solution: Named-Graph & Typed Anchoring

1. **Anchor to Exact Named Graphs**:
   Every repo has deterministic named graph IRIs:
   `GRAPH <https://repolex.ai/r/{org}/{repo}/lsp/{commit}>`
   When anchored, RocksDB seeks directly to the graph prefix in **<1.5 ms**.

2. **Order Triples by Selectivity**:
   Put the most specific triple pattern first so the query planner binds variables immediately.

3. **Pre-Warm RocksDB Cache**:
   Run `rlex warmup` before heavy traversal queries to fault SST index and filter blocks into RAM.
"#,
    },
    TopicDoc {
        topic: "prefixes",
        title: "Standard Repolex Ontology Namespaces",
        tags: &["prefixes", "vocabulary", "ontology", "rdf"],
        summary: "Reference of standard Repolex RDF prefixes for AST syntax, semantic symbols, LSP calls, and git metadata.",
        sparql_recipe: None,
        content: r#"# Standard Repolex Ontology Namespaces

Include these prefixes at the top of your SPARQL queries:

```sparql
PREFIX lx:       <https://repolex.ai/ontology/repolex/lsp-extension/>
PREFIX repolex:  <https://repolex.ai/ontology/repolex/>
PREFIX ast:      <https://repolex.ai/ontology/ast#>
PREFIX ast-x:    <https://repolex.ai/ontology/ast-x#>
PREFIX lsp:      <https://repolex.ai/ontology/lsp#>
PREFIX lsp-x:    <https://repolex.ai/ontology/lsp-x#>
PREFIX sem:      <https://repolex.ai/ontology/sem#>
PREFIX git:      <https://repolex.ai/ontology/extracts/gitpython-developers/GitPython/v3.1.46/core/>
PREFIX rdf:      <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
PREFIX rdfs:     <http://www.w3.org/2000/01/rdf-schema#>
PREFIX xsd:      <http://www.w3.org/2001/XMLSchema#>
```

### Key Predicates:
- `lx:resolutionSourceFile`: Source file where call originated (e.g. `"src/lib.rs"`)
- `lx:externalPackage`: Outgoing external package name (e.g. `"aho-corasick"`)
- `lx:callTarget`: URI of the resolved symbol or definition
- `repolex:packageName`: Package name literal in dependency graphs
- `ast-x:filePath`: Relative file path in AST syntax trees
"#,
    },
    TopicDoc {
        topic: "architecture",
        title: "Repolex 118M Quad Store Architecture",
        tags: &["architecture", "store", "oxigraph", "graphs"],
        summary: "Overview of named graph partitioning across AST blobs, LSP call edges, dependencies, and git structure.",
        sparql_recipe: None,
        content: r#"# Repolex Triplestore Architecture

Repolex stores code as a unified knowledge graph across four distinct layers:

1. **AST Blob & Syntax Graphs (69.5% / ~82.5M quads)**:
   - Named graph: `<https://repolex.ai/r/{org}/{repo}/blob/{blob_sha}>`
   - Fine-grained syntax nodes from Tree-Sitter (classes, methods, identifiers, parameters).

2. **Repolex CallSite & Semantic Graphs (15.9% / ~18.8M quads)**:
   - Function calls, invocations, and references within each file.

3. **LSP Cross-Repo Call Edges (2.7% / ~3.2M quads)**:
   - Named graph: `<https://repolex.ai/r/{org}/{repo}/lsp/{commit_sha}>`
   - High-fidelity call resolutions produced by Language Server Protocol (LSP).

4. **Git Structure & Manifests (11.9% / ~14.2M quads)**:
   - Commits, branches, tags, file trees, and package dependencies.
"#,
    },
];

pub fn run(topic_arg: Option<&str>, json: bool) -> Result<()> {
    if let Some(t) = topic_arg {
        let t_norm = t.trim().to_lowercase();

        let doc = TOPICS.iter().find(|d| {
            d.topic == t_norm
                || d.tags.iter().any(|tag| *tag == t_norm)
                || (t_norm == "optimization" && d.topic == "scan-guard")
        });

        if let Some(d) = doc {
            if json {
                println!("{}", serde_json::to_string_pretty(d)?);
            } else {
                println!("{}", d.content);
                if let Some(recipe) = d.sparql_recipe {
                    println!("\n### Copy-Paste SPARQL Recipe:\n```sparql\n{}\n```\n", recipe);
                }
            }
            return Ok(());
        } else {
            eprintln!("Unknown topic '{}'. Showing available topics:\n", t);
        }
    }

    // Default overview / topics list
    if json {
        let summary: Vec<_> = TOPICS
            .iter()
            .map(|d| {
                serde_json::json!({
                    "topic": d.topic,
                    "title": d.title,
                    "tags": d.tags,
                    "summary": d.summary,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }

    println!("================================================================================");
    println!("               REPOLEX IN-BINARY KNOWLEDGE BASE & RECIPES                       ");
    println!("================================================================================");
    println!("  Available documentation topics and SPARQL query recipes:\n");

    for d in TOPICS {
        println!("  • {:<14} {:<32} - {}", d.topic, format!("[{}]", d.title), d.summary);
    }

    println!("\nUsage:");
    println!("  rlex moreinfo <topic>        # View documentation & copy-paste SPARQL recipes");
    println!("  rlex moreinfo <topic> --json # Emit structured JSON for LLM agent tools");
    println!("================================================================================\n");

    Ok(())
}
