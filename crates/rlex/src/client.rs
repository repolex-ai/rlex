use anyhow::{bail, Context, Result};
use oxigraph::sparql::{QueryResults, Variable};
use oxigraph::store::Store;
use serde_json::Value;
use std::time::Instant;

use crate::config::Config;
use crate::query::{build_query, format_term};

#[derive(Debug, Clone)]
pub struct QueryResult {
    pub vars: Vec<String>,
    pub rows: Vec<Vec<String>>,
    #[allow(dead_code)]
    pub elapsed_ms: f64,
}

pub struct Client {
    endpoint: Option<String>,
    store: Option<Store>,
}

impl Client {
    /// Create a new query client.
    ///
    /// If an endpoint is explicitly provided, it will be used.
    /// Otherwise, attempts to open the local Oxigraph store at ~/.rlex/oxigraph.
    /// If the local store is locked or unavailable, falls back to the running
    /// background server at http://localhost:{port}/query.
    pub fn new(config: &Config, endpoint: Option<&str>) -> Result<Self> {
        if let Some(ep) = endpoint {
            return Ok(Self {
                endpoint: Some(ep.to_string()),
                store: None,
            });
        }

        // Try opening the local Oxigraph store read-only
        match Store::open_read_only(&config.paths.oxigraph) {
            Ok(store) => Ok(Self {
                endpoint: None,
                store: Some(store),
            }),
            Err(e) => {
                // Check if background server on default port is responding
                let server_url = format!("http://localhost:{}", config.server.sparql_port);
                let query_url = format!("{}/query", server_url);
                let check_client = reqwest::blocking::Client::builder()
                    .timeout(std::time::Duration::from_millis(500))
                    .build()?;

                if check_client.get(format!("{}/health", server_url)).send().is_ok() {
                    Ok(Self {
                        endpoint: Some(query_url),
                        store: None,
                    })
                } else {
                    bail!(
                        "Could not open local Oxigraph store at {} ({}) and background server at {} is offline.\n\
                         Run `rlex serve` to start the background server, or pass `--endpoint <url>`.",
                        config.paths.oxigraph.display(),
                        e,
                        server_url
                    );
                }
            }
        }
    }

    /// Is this client querying a remote HTTP endpoint?
    pub fn is_remote(&self) -> bool {
        self.endpoint.is_some()
    }

    /// Get current endpoint URL if remote
    pub fn endpoint_url(&self) -> Option<&str> {
        self.endpoint.as_deref()
    }

    /// Execute a SPARQL SELECT query and return structured results with timing
    pub fn query(&self, sparql: &str) -> Result<QueryResult> {
        let t0 = Instant::now();

        if let Some(ref ep) = self.endpoint {
            let http = reqwest::blocking::Client::new();
            let resp = http
                .post(ep)
                .header("Content-Type", "application/sparql-query")
                .header("Accept", "application/sparql-results+json")
                .body(sparql.to_string())
                .send()
                .with_context(|| format!("sending SPARQL query to {}", ep))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().unwrap_or_default();
                bail!("SPARQL endpoint {} returned HTTP {}: {}", ep, status, text);
            }

            let json: Value = resp
                .json()
                .with_context(|| "parsing SPARQL JSON results")?;
            let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;

            let vars: Vec<String> = json["head"]["vars"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            let mut rows = Vec::new();
            if let Some(bindings) = json["results"]["bindings"].as_array() {
                for b in bindings {
                    let mut row = Vec::new();
                    for v in &vars {
                        let val = b
                            .get(v)
                            .and_then(|node| node.get("value"))
                            .and_then(|val| val.as_str())
                            .unwrap_or("")
                            .to_string();
                        row.push(val);
                    }
                    rows.push(row);
                }
            }

            Ok(QueryResult {
                vars,
                rows,
                elapsed_ms,
            })
        } else if let Some(ref store) = self.store {
            let q = build_query(sparql, false)?;
            let results = store.query(q)?;
            let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;

            match results {
                QueryResults::Solutions(solutions) => {
                    let variables: Vec<Variable> = solutions.variables().to_vec();
                    let vars: Vec<String> =
                        variables.iter().map(|v| v.as_str().to_string()).collect();
                    let mut rows = Vec::new();
                    for solution in solutions {
                        let sol = solution?;
                        let row: Vec<String> = variables
                            .iter()
                            .map(|v| sol.get(v).map(format_term).unwrap_or_default())
                            .collect();
                        rows.push(row);
                    }
                    Ok(QueryResult {
                        vars,
                        rows,
                        elapsed_ms,
                    })
                }
                _ => bail!("Expected SPARQL SELECT solutions query"),
            }
        } else {
            bail!("No query backend available in client");
        }
    }
}
