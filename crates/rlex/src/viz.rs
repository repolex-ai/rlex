use anyhow::Result;
use std::process::Command;

use crate::config::Config;

pub fn run(config: &Config, port: u16, sparql_port: u16, stop: bool) -> Result<()> {
    if stop {
        // TODO: read pidfiles, kill both processes
        println!("Stopping viz + SPARQL servers...");
        return Ok(());
    }

    // Start SPARQL endpoint first (this blocks, so spawn in background)
    // TODO: for now, tell user to run rlex serve separately
    println!("Start the SPARQL endpoint first: rlex serve --port {}", sparql_port);
    println!("Then start the viz server.");

    // Spawn rlex-viz as background process
    let _child = Command::new("rlex-viz")
        .args([
            "--port", &port.to_string(),
            "--sparql-url", &format!("http://localhost:{}", sparql_port),
        ])
        .spawn()?;

    println!("Viz UI running at http://localhost:{}", port);
    println!("SPARQL endpoint at http://localhost:{}", sparql_port);

    Ok(())
}
