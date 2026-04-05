use anyhow::{Context, Result};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use oxigraph::sparql::{QueryResults, Variable};
use oxigraph::store::Store;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::CorsLayer;

use crate::config::Config;

struct AppState {
    store: Store,
    catalog_path: PathBuf,
    viz_dir: Option<PathBuf>,
}

pub fn run(config: &Config, port: u16, viz_dir: Option<&str>) -> Result<()> {
    let store = Store::open_read_only(&config.paths.oxigraph)
        .with_context(|| format!("opening oxigraph store at {}", config.paths.oxigraph.display()))?;

    let viz_path = viz_dir.map(PathBuf::from);

    let state = Arc::new(AppState {
        store,
        catalog_path: config.paths.root.join("catalog.json"),
        viz_dir: viz_path.clone(),
    });

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let app = Router::new()
            .route("/query", get(sparql_get).post(sparql_post))
            .route("/sparql", get(sparql_get).post(sparql_post))
            .route("/api/catalog", get(catalog))
            .route("/api/repos", get(repos))
            .route("/api/graph", get(graph))
            .route("/api/cluster", get(cluster))
            .route("/health", get(health));

        // Serve index.html from viz dir if provided
        let app = if viz_path.is_some() {
            app.route("/", get(index_html))
                .route("/index.html", get(index_html))
        } else {
            app
        };

        let app = app
            .layer(CorsLayer::permissive())
            .with_state(state);

        let addr = format!("127.0.0.1:{}", port);
        println!("rlex serve on http://{}", addr);
        println!("  SPARQL:  http://{}/query", addr);
        println!("  Catalog: http://{}/api/catalog", addr);
        println!("  Repos:   http://{}/api/repos", addr);
        println!("  Graph:   http://{}/api/graph?repo=org/repo", addr);
        if viz_dir.is_some() {
            println!("  Viz UI:  http://{}/", addr);
        }

        let listener = tokio::net::TcpListener::bind(&addr).await
            .expect("failed to bind");
        axum::serve(listener, app).await.expect("server error");
    });

    Ok(())
}

// ── Static file serving ──

async fn index_html(State(state): State<Arc<AppState>>) -> Response {
    if let Some(ref dir) = state.viz_dir {
        let path = dir.join("index.html");
        match std::fs::read_to_string(&path) {
            Ok(html) => Html(html).into_response(),
            Err(_) => (StatusCode::NOT_FOUND, "index.html not found").into_response(),
        }
    } else {
        (StatusCode::NOT_FOUND, "No viz directory configured").into_response()
    }
}

// ── SPARQL endpoint ──

#[derive(Deserialize)]
struct SparqlQuery {
    query: String,
}

async fn sparql_get(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SparqlQuery>,
) -> Response {
    execute_sparql(&state.store, &params.query)
}

async fn sparql_post(
    State(state): State<Arc<AppState>>,
    body: String,
) -> Response {
    let sparql = if body.starts_with("query=") {
        urlencoding::decode(&body[6..])
            .unwrap_or_default()
            .to_string()
    } else {
        body
    };
    execute_sparql(&state.store, &sparql)
}

// ── Viz API endpoints ──

#[derive(Deserialize)]
struct RepoQuery {
    repo: String,
}

/// GET /api/repos — list repos that have data loaded in the store
async fn repos(State(state): State<Arc<AppState>>) -> Response {
    let sparql = "SELECT DISTINCT ?g WHERE { GRAPH ?g { ?s ?p ?o } }";
    #[allow(deprecated)]
    let results = match state.store.query(sparql) {
        Ok(r) => r,
        Err(e) => return error_json(e.to_string()),
    };

    let mut repo_set = std::collections::BTreeSet::new();
    if let QueryResults::Solutions(solutions) = results {
        for solution in solutions.flatten() {
            if let Some(oxigraph::model::Term::NamedNode(n)) = solution.get("g") {
                let uri = n.as_str();
                // Extract org/repo from graph URIs like:
                // https://repolex.ai/r/{org}/{repo}/...
                // https://repolex.ai/data/{org}/{repo}/...
                for prefix in ["https://repolex.ai/r/", "https://repolex.ai/data/"] {
                    if let Some(rest) = uri.strip_prefix(prefix) {
                        let parts: Vec<&str> = rest.splitn(3, '/').collect();
                        if parts.len() >= 2 {
                            repo_set.insert(format!("{}/{}", parts[0], parts[1]));
                        }
                    }
                }
            }
        }
    }

    let repos: Vec<&str> = repo_set.iter().map(|s| s.as_str()).collect();
    Json(serde_json::json!(repos)).into_response()
}

/// GET /api/graph?repo=org/repo — functions + edges for force/complexity views
async fn graph(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RepoQuery>,
) -> Response {
    let repo = &params.repo;

    // Find the AST graph URI
    let ast_graph = match find_graph(&state.store, repo, "/ast/") {
        Some(g) => g,
        None => return Json(serde_json::json!({
            "repo": repo,
            "error": "No AST graph found for this repo. Download with repolex data first.",
            "nodes": [],
            "links": [],
        })).into_response(),
    };

    // Get functions
    let fn_query = format!(
        r#"SELECT ?fn ?name ?complexity ?lines WHERE {{
            GRAPH <{ast_graph}> {{
                ?fn a <https://repolex.ai/ontology/repolex/ast-extension/FunctionDefinition> ;
                    <http://www.w3.org/2000/01/rdf-schema#label> ?name .
                OPTIONAL {{ ?fn <https://repolex.ai/ontology/repolex/ast-extension/cyclomaticComplexity> ?complexity }}
                OPTIONAL {{ ?fn <https://repolex.ai/ontology/repolex/ast-extension/lineCount> ?lines }}
            }}
        }} ORDER BY DESC(?complexity) LIMIT 300"#
    );

    let nodes = query_to_rows(&state.store, &fn_query);

    // Process nodes — extract file path from function URI blob hash
    let mut node_ids = std::collections::HashSet::new();
    let processed_nodes: Vec<serde_json::Value> = nodes.iter().map(|row| {
        let fn_uri = row.get("fn").map(|v| v.as_str()).unwrap_or("");
        let name = row.get("name").map(|v| v.as_str()).unwrap_or("");
        let clean_name = clean_fn_name(name);

        // Extract file info from the function URI fragment
        // URI: .../blob/{hash}#name_startRow_startCol_endRow_endCol
        // We use the blob hash portion of the path as a group key
        let blob_hash = extract_blob_hash(fn_uri).unwrap_or("unknown");

        node_ids.insert(fn_uri.to_string());

        serde_json::json!({
            "id": fn_uri,
            "name": clean_name,
            "fullName": name,
            "path": blob_hash,
            "file": blob_hash,
            "complexity": row.get("complexity").map(|v| v.as_str()).and_then(|s| s.parse::<i64>().ok()).unwrap_or(1),
            "lines": row.get("lines").map(|v| v.as_str()).and_then(|s| s.parse::<i64>().ok()).unwrap_or(1),
        })
    }).collect();

    let links: Vec<serde_json::Value> = Vec::new();
    // TODO: edge matching via callsite position overlap — expensive, optimize later

    Json(serde_json::json!({
        "repo": repo,
        "graphUri": ast_graph,
        "nodes": processed_nodes,
        "links": links,
        "stats": {
            "nodeCount": processed_nodes.len(),
            "edgeCount": links.len(),
        }
    })).into_response()
}

/// GET /api/cluster?repo=org/repo — all functions for cluster/hulls views
async fn cluster(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RepoQuery>,
) -> Response {
    // Same as graph but with no LIMIT
    let repo = &params.repo;

    let ast_graph = match find_graph(&state.store, repo, "/ast/") {
        Some(g) => g,
        None => return Json(serde_json::json!({
            "repo": repo,
            "error": "No AST graph found",
            "nodes": [],
            "links": [],
            "files": [],
        })).into_response(),
    };

    let fn_query = format!(
        r#"SELECT ?fn ?name ?complexity ?lines WHERE {{
            GRAPH <{ast_graph}> {{
                ?fn a <https://repolex.ai/ontology/repolex/ast-extension/FunctionDefinition> ;
                    <http://www.w3.org/2000/01/rdf-schema#label> ?name .
                OPTIONAL {{ ?fn <https://repolex.ai/ontology/repolex/ast-extension/cyclomaticComplexity> ?complexity }}
                OPTIONAL {{ ?fn <https://repolex.ai/ontology/repolex/ast-extension/lineCount> ?lines }}
            }}
        }} ORDER BY DESC(?complexity)"#
    );

    let nodes = query_to_rows(&state.store, &fn_query);

    let mut files_set = std::collections::BTreeSet::new();

    let processed_nodes: Vec<serde_json::Value> = nodes.iter().map(|row| {
        let fn_uri = row.get("fn").map(|v| v.as_str()).unwrap_or("");
        let name = row.get("name").map(|v| v.as_str()).unwrap_or("");
        let clean_name = clean_fn_name(name);
        let blob_hash = extract_blob_hash(fn_uri).unwrap_or("unknown");

        files_set.insert(blob_hash.to_string());

        serde_json::json!({
            "id": fn_uri,
            "name": clean_name,
            "fullName": name,
            "path": blob_hash,
            "file": blob_hash,
            "complexity": row.get("complexity").map(|v| v.as_str()).and_then(|s| s.parse::<i64>().ok()).unwrap_or(1),
            "lines": row.get("lines").map(|v| v.as_str()).and_then(|s| s.parse::<i64>().ok()).unwrap_or(1),
        })
    }).collect();

    let files: Vec<&str> = files_set.iter().map(|s| s.as_str()).collect();

    Json(serde_json::json!({
        "repo": repo,
        "graphUri": ast_graph,
        "nodes": processed_nodes,
        "links": [],
        "files": files,
        "stats": {
            "nodeCount": processed_nodes.len(),
            "fileCount": files.len(),
        }
    })).into_response()
}

// ── Helpers ──

fn find_graph(store: &Store, repo: &str, contains: &str) -> Option<String> {
    let sparql = format!(
        "SELECT DISTINCT ?g WHERE {{ GRAPH ?g {{ ?s ?p ?o }} FILTER(CONTAINS(STR(?g), '{}') && CONTAINS(STR(?g), '{}')) }} LIMIT 1",
        repo, contains
    );
    #[allow(deprecated)]
    let results = store.query(&sparql).ok()?;
    if let QueryResults::Solutions(solutions) = results {
        for solution in solutions.flatten() {
            if let Some(oxigraph::model::Term::NamedNode(n)) = solution.get("g") {
                return Some(n.as_str().to_string());
            }
        }
    }
    None
}

fn build_blob_map(store: &Store, filetree_graph: &str) -> std::collections::HashMap<String, String> {
    let sparql = format!(
        r#"SELECT ?blob ?path WHERE {{
            GRAPH <{filetree_graph}> {{
                ?blob <https://repolex.ai/ontology/repolex/filePath> ?path
            }}
        }}"#
    );
    let rows = query_to_rows(store, &sparql);
    let mut map = std::collections::HashMap::new();
    for row in &rows {
        let blob_uri = row.get("blob").map(|v| v.as_str()).unwrap_or("");
        let path = row.get("path").map(|v| v.as_str()).unwrap_or("");
        if let Some(hash) = extract_blob_hash(blob_uri) {
            map.insert(hash.to_string(), path.to_string());
        }
    }
    map
}

fn query_to_rows(store: &Store, sparql: &str) -> Vec<std::collections::HashMap<String, String>> {
    #[allow(deprecated)]
    let results = match store.query(sparql) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let mut rows = Vec::new();
    if let QueryResults::Solutions(solutions) = results {
        let vars: Vec<Variable> = solutions.variables().to_vec();
        for solution in solutions.flatten() {
            let mut row = std::collections::HashMap::new();
            for var in &vars {
                if let Some(term) = solution.get(var) {
                    let val = match term {
                        oxigraph::model::Term::NamedNode(n) => n.as_str().to_string(),
                        oxigraph::model::Term::Literal(l) => l.value().to_string(),
                        oxigraph::model::Term::BlankNode(b) => b.as_str().to_string(),
                        _ => term.to_string(),
                    };
                    row.insert(var.as_str().to_string(), val);
                }
            }
            rows.push(row);
        }
    }
    rows
}

fn clean_fn_name(name: &str) -> String {
    // Strip position suffix: name_startRow_startCol_endRow_endCol
    let re = regex::Regex::new(r"_\d+_\d+_\d+_\d+$").unwrap();
    re.replace(name, "").to_string()
}

fn extract_blob_hash(uri: &str) -> Option<&str> {
    // Extract blob hash from URIs like .../blob/{hash}/... or .../blob/{hash}#...
    let idx = uri.find("/blob/")?;
    let rest = &uri[idx + 6..];
    let end = rest.find(|c: char| c == '/' || c == '#').unwrap_or(rest.len());
    Some(&rest[..end])
}

fn parse_fn_position(uri: &str) -> Option<(String, i64, i64)> {
    // URI: .../blob/{hash}#name_startRow_startCol_endRow_endCol
    let blob = extract_blob_hash(uri)?.to_string();
    let fragment = uri.rsplit('#').next()?;
    let parts: Vec<&str> = fragment.rsplitn(5, '_').collect();
    if parts.len() >= 4 {
        let start: i64 = parts[3].parse().ok()?;
        let end: i64 = parts[1].parse().ok()?;
        Some((blob, start, end))
    } else {
        None
    }
}

fn error_json(msg: String) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": msg}))).into_response()
}

fn execute_sparql(store: &Store, sparql: &str) -> Response {
    #[allow(deprecated)]
    let results = match store.query(sparql) {
        Ok(r) => r,
        Err(e) => return error_json(e.to_string()),
    };

    match results {
        QueryResults::Solutions(solutions) => {
            let variables: Vec<Variable> = solutions.variables().to_vec();
            let var_names: Vec<String> = variables.iter().map(|v| v.as_str().to_string()).collect();

            let mut bindings = Vec::new();
            for solution in solutions {
                match solution {
                    Ok(s) => {
                        let mut row = serde_json::Map::new();
                        for var in &variables {
                            if let Some(term) = s.get(var) {
                                row.insert(var.as_str().to_string(), term_to_json(term));
                            }
                        }
                        bindings.push(serde_json::Value::Object(row));
                    }
                    Err(e) => return error_json(e.to_string()),
                }
            }

            Json(serde_json::json!({
                "head": { "vars": var_names },
                "results": { "bindings": bindings }
            })).into_response()
        }

        QueryResults::Boolean(result) => {
            Json(serde_json::json!({ "head": {}, "boolean": result })).into_response()
        }

        QueryResults::Graph(triples) => {
            let mut nt = String::new();
            for triple in triples {
                match triple {
                    Ok(t) => nt.push_str(&format!("{}\n", t)),
                    Err(e) => return error_json(e.to_string()),
                }
            }
            (StatusCode::OK, [("content-type", "application/n-triples")], nt).into_response()
        }
    }
}

fn term_to_json(term: &oxigraph::model::Term) -> serde_json::Value {
    match term {
        oxigraph::model::Term::NamedNode(n) => {
            serde_json::json!({"type": "uri", "value": n.as_str()})
        }
        oxigraph::model::Term::BlankNode(b) => {
            serde_json::json!({"type": "bnode", "value": b.as_str()})
        }
        oxigraph::model::Term::Literal(l) => {
            let mut obj = serde_json::Map::new();
            obj.insert("type".into(), "literal".into());
            obj.insert("value".into(), l.value().into());
            if let Some(lang) = l.language() {
                obj.insert("xml:lang".into(), lang.into());
            } else if l.datatype() != oxigraph::model::vocab::xsd::STRING {
                obj.insert("datatype".into(), l.datatype().as_str().into());
            }
            serde_json::Value::Object(obj)
        }
        #[allow(unreachable_patterns)]
        _ => serde_json::json!({"type": "unknown", "value": term.to_string()}),
    }
}

async fn catalog(State(state): State<Arc<AppState>>) -> Response {
    match std::fs::read_to_string(&state.catalog_path) {
        Ok(contents) => (StatusCode::OK, [("content-type", "application/json")], contents).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "catalog.json not found"}))).into_response(),
    }
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok", "service": "rlex"}))
}
