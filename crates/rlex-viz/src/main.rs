use actix_web::{web, App, HttpServer, HttpResponse};
use clap::Parser;
use anyhow::Result;

#[derive(Parser)]
#[command(name = "rlex-viz", about = "Visualization server for repolex knowledge graphs")]
struct Cli {
    /// Port to serve on
    #[arg(short, long, default_value = "3000")]
    port: u16,

    /// SPARQL endpoint URL
    #[arg(long, default_value = "http://localhost:7878")]
    sparql_url: String,
}

struct AppState {
    sparql_url: String,
}

async fn index() -> HttpResponse {
    // TODO: serve embedded HTML/JS/CSS with D3/Mermaid viz
    HttpResponse::Ok()
        .content_type("text/html")
        .body("<html><body><h1>rlex-viz</h1><p>TODO: visualization UI</p></body></html>")
}

async fn health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({"status": "ok"}))
}

#[actix_web::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    println!("rlex-viz starting on :{}", cli.port);
    println!("SPARQL endpoint: {}", cli.sparql_url);

    let state = web::Data::new(AppState {
        sparql_url: cli.sparql_url,
    });

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .route("/", web::get().to(index))
            .route("/health", web::get().to(health))
    })
    .bind(format!("localhost:{}", cli.port))?
    .run()
    .await?;

    Ok(())
}
