use anyhow::{Context, Result};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use oxigraph::sparql::{QueryResults, Variable};
use oxigraph::store::Store;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

use crate::config::Config;

struct AppState {
    store: Store,
    catalog_path: std::path::PathBuf,
}

pub fn run(config: &Config, port: u16) -> Result<()> {
    let store = Store::open_read_only(&config.paths.oxigraph)
        .with_context(|| format!("opening oxigraph store at {}", config.paths.oxigraph.display()))?;

    let state = Arc::new(AppState {
        store,
        catalog_path: config.paths.root.join("catalog.json"),
    });

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let app = Router::new()
            .route("/query", get(sparql_get).post(sparql_post))
            .route("/sparql", get(sparql_get).post(sparql_post))
            .route("/api/catalog", get(catalog))
            .route("/health", get(health))
            .layer(CorsLayer::permissive())
            .with_state(state);

        let addr = format!("127.0.0.1:{}", port);
        println!("rlex serve on http://{}", addr);
        println!("  SPARQL endpoint: http://{}/query", addr);
        println!("  Catalog API:     http://{}/api/catalog", addr);
        println!("  Health:          http://{}/health", addr);

        let listener = tokio::net::TcpListener::bind(&addr).await
            .expect("failed to bind");
        axum::serve(listener, app).await.expect("server error");
    });

    Ok(())
}

// GET /query?query=SELECT...
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

// POST /query with body as SPARQL string
async fn sparql_post(
    State(state): State<Arc<AppState>>,
    body: String,
) -> Response {
    // Support both raw SPARQL body and form-encoded query=...
    let sparql = if body.starts_with("query=") {
        urlencoding::decode(&body[6..])
            .unwrap_or_default()
            .to_string()
    } else {
        body
    };
    execute_sparql(&state.store, &sparql)
}

fn execute_sparql(store: &Store, sparql: &str) -> Response {
    #[allow(deprecated)]
    let results = match store.query(sparql) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response();
        }
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
                                row.insert(
                                    var.as_str().to_string(),
                                    term_to_json(term),
                                );
                            }
                        }
                        bindings.push(serde_json::Value::Object(row));
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(serde_json::json!({"error": e.to_string()})),
                        )
                            .into_response();
                    }
                }
            }

            // SPARQL 1.1 Query Results JSON Format
            let response = serde_json::json!({
                "head": { "vars": var_names },
                "results": { "bindings": bindings }
            });

            Json(response).into_response()
        }

        QueryResults::Boolean(result) => {
            let response = serde_json::json!({
                "head": {},
                "boolean": result
            });
            Json(response).into_response()
        }

        QueryResults::Graph(triples) => {
            // Return as N-Triples for CONSTRUCT/DESCRIBE
            let mut nt = String::new();
            for triple in triples {
                match triple {
                    Ok(t) => {
                        nt.push_str(&format!("{}\n", t));
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(serde_json::json!({"error": e.to_string()})),
                        )
                            .into_response();
                    }
                }
            }
            (StatusCode::OK, [("content-type", "application/n-triples")], nt).into_response()
        }
    }
}

fn term_to_json(term: &oxigraph::model::Term) -> serde_json::Value {
    match term {
        oxigraph::model::Term::NamedNode(n) => {
            serde_json::json!({
                "type": "uri",
                "value": n.as_str()
            })
        }
        oxigraph::model::Term::BlankNode(b) => {
            serde_json::json!({
                "type": "bnode",
                "value": b.as_str()
            })
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
        Ok(contents) => (
            StatusCode::OK,
            [("content-type", "application/json")],
            contents,
        )
            .into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "catalog.json not found. Run `rlex sync` first."})),
        )
            .into_response(),
    }
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok", "service": "rlex"}))
}
