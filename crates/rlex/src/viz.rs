use anyhow::Result;
use std::process::Command;

pub fn run(port: u16, sparql_port: u16, stop: bool) -> Result<()> {
    if stop {
        // TODO: read pidfiles, kill both processes
        println!("Stopping viz + SPARQL servers...");
        return Ok(());
    }

    // Start SPARQL endpoint first
    super::serve::run(sparql_port, false)?;

    // Spawn rlex-viz as background process
    let _child = Command::new("rlex-viz")
        .args([
            "--port", &port.to_string(),
            "--sparql-url", &format!("http://localhost:{}", sparql_port),
        ])
        .spawn()?;

    // TODO: write pidfile for --stop
    println!("Viz UI running at http://localhost:{}", port);
    println!("SPARQL endpoint at http://localhost:{}", sparql_port);

    Ok(())
}
