use anyhow::{Context, Result};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json, Response},
    routing::get,
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

/// Spawn rlex serve as a background process, write pidfile, optionally open browser
pub fn background(config: &Config, port: u16, viz_dir: Option<&str>, open_browser: bool) -> Result<()> {
    let pidfile = config.paths.root.join("serve.pid");

    // Check if already running
    if pidfile.exists() {
        if let Ok(pid_str) = std::fs::read_to_string(&pidfile) {
            let pid = pid_str.trim();
            // Check if process is alive
            let status = std::process::Command::new("kill")
                .args(["-0", pid])
                .output();
            if status.map(|s| s.status.success()).unwrap_or(false) {
                let url = format!("http://localhost:{}", port);
                println!("rlex serve already running (pid {})", pid);
                println!("  {}", url);
                if open_browser {
                    let _ = open_url(&url);
                }
                return Ok(());
            }
            // Stale pidfile, remove it
            let _ = std::fs::remove_file(&pidfile);
        }
    }

    // Build args for the background process
    let exe = std::env::current_exe().context("finding rlex binary")?;
    let mut args = vec![
        "serve".to_string(),
        "--foreground".to_string(),
        "--port".to_string(),
        port.to_string(),
        "--no-browser".to_string(),
    ];
    if let Some(dir) = viz_dir {
        args.push("--viz-dir".to_string());
        args.push(dir.to_string());
    }

    let child = std::process::Command::new(&exe)
        .args(&args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("spawning background rlex serve")?;

    let pid = child.id();
    std::fs::write(&pidfile, pid.to_string())
        .context("writing pidfile")?;

    let url = format!("http://localhost:{}", port);
    println!("rlex serve started (pid {})", pid);
    println!("  {}", url);
    println!("  pidfile: {}", pidfile.display());
    println!("  stop: rlex serve --stop");

    // Give server a moment to start
    std::thread::sleep(std::time::Duration::from_millis(500));

    if open_browser {
        if let Some(viz) = viz_dir {
            println!("  opening browser...");
            let _ = open_url(&url);
        } else {
            println!("  SPARQL endpoint ready (no viz dir, skipping browser)");
        }
    }

    Ok(())
}

/// Stop a running background server
pub fn stop(config: &Config) -> Result<()> {
    let pidfile = config.paths.root.join("serve.pid");
    if !pidfile.exists() {
        println!("No running server found.");
        return Ok(());
    }

    let pid_str = std::fs::read_to_string(&pidfile)?;
    let pid = pid_str.trim();

    let status = std::process::Command::new("kill")
        .arg(pid)
        .output();

    match status {
        Ok(s) if s.status.success() => println!("Stopped rlex serve (pid {})", pid),
        _ => println!("Process {} not running (stale pidfile)", pid),
    }

    std::fs::remove_file(&pidfile)?;
    Ok(())
}

fn open_url(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    std::process::Command::new("open").arg(url).spawn()?;
    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open").arg(url).spawn()?;
    Ok(())
}

/// Run server in foreground (blocking)
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
        println!("  Health:  http://{}/health", addr);
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
