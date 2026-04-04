use anyhow::{bail, Result};
use std::process::Command;

pub fn run(port: u16, stop: bool) -> Result<()> {
    if stop {
        // TODO: read pidfile, kill process
        println!("Stopping SPARQL endpoint...");
        return Ok(());
    }

    let store_path = dirs::home_dir()
        .expect("could not determine home directory")
        .join(".rlex/oxigraph");

    println!("Starting SPARQL endpoint on :{}", port);

    let _child = Command::new("oxigraph")
        .args([
            "serve-read-only",
            "--location", &store_path.to_string_lossy(),
            "--bind", &format!("localhost:{}", port),
        ])
        .spawn()?;

    // TODO: write pidfile for --stop
    println!("SPARQL endpoint running at http://localhost:{}", port);

    Ok(())
}
