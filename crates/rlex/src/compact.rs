use anyhow::{bail, Context, Result};
use oxigraph::store::Store;
use serde::Serialize;
use std::fs;
use std::path::Path;
use std::time::Instant;

use crate::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreMetrics {
    pub sst_files: usize,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompactReport {
    pub store_path: String,
    pub sst_before: usize,
    pub sst_after: usize,
    pub sst_delta: i64,
    pub bytes_before: u64,
    pub bytes_after: u64,
    pub bytes_delta: i64,
    pub elapsed_ms: f64,
}

/// Count `.sst` files and total byte footprint in a RocksDB store directory.
pub fn measure_store(store_path: &Path) -> Result<StoreMetrics> {
    let mut sst_files = 0usize;
    let mut total_bytes = 0u64;

    for entry in fs::read_dir(store_path)
        .with_context(|| format!("reading store directory {}", store_path.display()))?
    {
        let entry = entry?;
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !meta.is_file() {
            continue;
        }
        total_bytes = total_bytes.saturating_add(meta.len());
        if let Some(ext) = entry.path().extension() {
            if ext == "sst" {
                sst_files += 1;
            }
        }
    }

    Ok(StoreMetrics {
        sst_files,
        total_bytes,
    })
}

/// Open the Oxigraph store read-write, run RocksDB compaction via `Store::optimize()`,
/// and return before/after `.sst` counts, disk footprint, and elapsed time.
pub fn compact_store(store_path: &Path) -> Result<CompactReport> {
    if !store_path.exists() {
        bail!(
            "Oxigraph store does not exist at {}",
            store_path.display()
        );
    }

    let before = measure_store(store_path)?;
    let start = Instant::now();

    let store = Store::open(store_path).with_context(|| {
        format!(
            "opening oxigraph store at {} for compaction (ensure no other writer such as `rlex load` or `rlex serve` holds the lock)",
            store_path.display()
        )
    })?;

    store
        .optimize()
        .context("compacting oxigraph RocksDB store via store.optimize()")?;

    // Drop the store so RocksDB flushes and releases handles before re-measuring.
    drop(store);

    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    let after = measure_store(store_path)?;

    Ok(CompactReport {
        store_path: store_path.display().to_string(),
        sst_before: before.sst_files,
        sst_after: after.sst_files,
        sst_delta: after.sst_files as i64 - before.sst_files as i64,
        bytes_before: before.total_bytes,
        bytes_after: after.total_bytes,
        bytes_delta: after.total_bytes as i64 - before.total_bytes as i64,
        elapsed_ms,
    })
}

fn format_bytes(bytes: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

fn format_bytes_delta(delta: i64) -> String {
    let sign = if delta > 0 { "+" } else if delta < 0 { "-" } else { "" };
    format!("{}{}", sign, format_bytes(delta.unsigned_abs()))
}

pub fn run(config: &Config, json: bool) -> Result<()> {
    let store_path = &config.paths.oxigraph;
    if !json {
        println!(
            "Compacting Oxigraph RocksDB store at {}...",
            store_path.display()
        );
    }

    let report = compact_store(store_path)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!("================================================================================");
    println!("               REPOLEX TRIPLESTORE COMPACTION REPORT                            ");
    println!("================================================================================");
    println!("  Store Location:   {}", report.store_path);
    println!(
        "  SST Files:        {} -> {} ({:+})",
        report.sst_before, report.sst_after, report.sst_delta
    );
    println!(
        "  Disk Footprint:   {} -> {} ({})",
        format_bytes(report.bytes_before),
        format_bytes(report.bytes_after),
        format_bytes_delta(report.bytes_delta)
    );
    println!("  Elapsed:          {:.2} s ({:.1} ms)", report.elapsed_ms / 1000.0, report.elapsed_ms);
    println!("================================================================================");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxigraph::model::{GraphName, NamedNode, Quad};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "rlex_compact_{}_{}_{}",
            label,
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_compact_store_preserves_quads_and_reports_metrics() {
        let dir = unique_temp_dir("ok");

        // Write across multiple open/close cycles to produce multiple L0 SST files.
        let total_quads = 60usize;
        for batch in 0..3usize {
            let store = Store::open(&dir).expect("open store for batch write");
            let pred = NamedNode::new("https://repolex.ai/ontology/test/edge").unwrap();
            let graph = GraphName::NamedNode(
                NamedNode::new(format!("https://repolex.ai/r/test/repo/lsp/batch{}", batch))
                    .unwrap(),
            );
            for i in 0..20usize {
                let idx = batch * 20 + i;
                let s = NamedNode::new(format!("https://repolex.ai/n/s{}", idx)).unwrap();
                let o = NamedNode::new(format!("https://repolex.ai/n/o{}", idx)).unwrap();
                store.insert(&Quad::new(s, pred.clone(), o, graph.clone())).unwrap();
            }
            store.flush().unwrap();
        }

        let report = compact_store(&dir).expect("compact_store should succeed");
        assert!(report.sst_before >= 1, "expected >=1 SST before compaction");
        assert!(report.sst_after >= 1, "expected >=1 SST after compaction");
        assert!(
            report.sst_after <= report.sst_before,
            "compaction should not increase SST count ({} -> {})",
            report.sst_before,
            report.sst_after
        );
        assert!(report.bytes_before > 0);
        assert!(report.bytes_after > 0);

        // Verify all quads survived compaction intact.
        let store = Store::open_read_only(&dir).expect("reopen store read-only");
        assert_eq!(store.len().unwrap(), total_quads);
        drop(store);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_compact_store_missing_path_errors() {
        let missing = std::env::temp_dir().join(format!(
            "rlex_compact_nonexistent_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&missing);
        let err = compact_store(&missing).unwrap_err();
        assert!(
            err.to_string().contains("does not exist"),
            "unexpected error: {}",
            err
        );
    }
}
