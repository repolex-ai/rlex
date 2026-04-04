use anyhow::{bail, Context, Result};
use oxigraph::io::RdfFormat;
use oxigraph::sparql::{QueryResults, Variable};
use oxigraph::store::Store;
use std::io::BufWriter;

use crate::config::Config;

pub fn run(config: &Config, sparql: &str, format: &str) -> Result<()> {
    let store = Store::open_read_only(&config.paths.oxigraph)
        .with_context(|| format!("opening oxigraph store at {}", config.paths.oxigraph.display()))?;

    let results = store.query(sparql)
        .with_context(|| "executing SPARQL query")?;

    match results {
        QueryResults::Solutions(solutions) => {
            let variables: Vec<Variable> = solutions.variables().to_vec();

            let mut rows: Vec<Vec<String>> = Vec::new();
            for solution in solutions {
                let solution = solution.context("reading query result")?;
                let row: Vec<String> = variables
                    .iter()
                    .map(|var| {
                        solution.get(var)
                            .map(|term| format_term(term))
                            .unwrap_or_default()
                    })
                    .collect();
                rows.push(row);
            }

            let var_names: Vec<&str> = variables.iter().map(|v| v.as_str()).collect();

            match format {
                "table" => print_table(&var_names, &rows),
                "csv" => print_csv(&var_names, &rows),
                "tsv" => print_tsv(&var_names, &rows),
                "json" => print_json(&var_names, &rows)?,
                _ => bail!("Unknown format '{}'. Supported: table, csv, tsv, json", format),
            }

            eprintln!("\n{} results", rows.len());
        }

        QueryResults::Boolean(result) => {
            println!("{}", result);
        }

        QueryResults::Graph(triples) => {
            let stdout = std::io::stdout();
            let mut writer = BufWriter::new(stdout.lock());

            let rdf_format = match format {
                "turtle" | "ttl" => RdfFormat::Turtle,
                "ntriples" | "nt" => RdfFormat::NTriples,
                "nquads" | "nq" => RdfFormat::NQuads,
                _ => RdfFormat::Turtle,
            };

            let mut serializer = oxigraph::io::RdfSerializer::from_format(rdf_format)
                .for_writer(&mut writer);

            let mut count = 0u64;
            for triple in triples {
                let triple = triple.context("reading CONSTRUCT result")?;
                serializer.serialize_triple(&triple)
                    .context("serializing triple")?;
                count += 1;
            }
            serializer.finish()
                .context("finishing serialization")?;

            eprintln!("\n{} triples", count);
        }
    }

    Ok(())
}

fn format_term(term: &oxigraph::model::Term) -> String {
    match term {
        oxigraph::model::Term::NamedNode(n) => {
            let iri = n.as_str();
            // Try to compact common prefixes
            compact_iri(iri)
        }
        oxigraph::model::Term::BlankNode(b) => format!("_:{}", b.as_str()),
        oxigraph::model::Term::Literal(l) => {
            if l.datatype() == oxigraph::model::vocab::xsd::STRING {
                l.value().to_string()
            } else if l.datatype() == oxigraph::model::vocab::xsd::INTEGER
                || l.datatype() == oxigraph::model::vocab::xsd::DECIMAL
                || l.datatype() == oxigraph::model::vocab::xsd::DOUBLE
                || l.datatype() == oxigraph::model::vocab::xsd::BOOLEAN
            {
                l.value().to_string()
            } else if let Some(lang) = l.language() {
                format!("{}@{}", l.value(), lang)
            } else {
                format!("{}^^{}", l.value(), compact_iri(l.datatype().as_str()))
            }
        }
        #[allow(unreachable_patterns)]
        _ => term.to_string(),
    }
}

fn compact_iri(iri: &str) -> String {
    // Common repolex prefixes
    let prefixes: &[(&str, &str)] = &[
        ("https://repolex.ai/ontology/ast-x#", "ast-x:"),
        ("https://repolex.ai/ontology/ast#", "ast:"),
        ("https://repolex.ai/ontology/lsp-x#", "lsp-x:"),
        ("https://repolex.ai/ontology/lsp#", "lsp:"),
        ("https://repolex.ai/ontology/sem#", "sem:"),
        ("https://repolex.ai/ontology/extracts/gitpython-developers/GitPython/v3.1.46/core/", "git:"),
        ("https://repolex.ai/ontology/repolex/security/", "security:"),
        ("https://repolex.ai/r/", "r:"),
        ("http://www.w3.org/1999/02/22-rdf-syntax-ns#", "rdf:"),
        ("http://www.w3.org/2000/01/rdf-schema#", "rdfs:"),
        ("http://www.w3.org/2001/XMLSchema#", "xsd:"),
        ("http://www.w3.org/ns/shacl#", "sh:"),
    ];

    for (prefix, short) in prefixes {
        if let Some(local) = iri.strip_prefix(prefix) {
            return format!("{}{}", short, local);
        }
    }

    iri.to_string()
}

fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    if rows.is_empty() {
        println!("(no results)");
        return;
    }

    // Calculate column widths
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(cell.len().min(80));
            }
        }
    }

    // Header
    let header_line: Vec<String> = headers
        .iter()
        .zip(&widths)
        .map(|(h, w)| format!("{:<width$}", h, width = *w))
        .collect();
    println!("{}", header_line.join("  "));

    let separator: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    println!("{}", separator.join("  "));

    // Rows
    for row in rows {
        let cells: Vec<String> = row
            .iter()
            .zip(&widths)
            .map(|(cell, w)| {
                if cell.len() > 80 {
                    format!("{:<width$}", format!("{}...", &cell[..77]), width = *w)
                } else {
                    format!("{:<width$}", cell, width = *w)
                }
            })
            .collect();
        println!("{}", cells.join("  "));
    }
}

fn print_csv(headers: &[&str], rows: &[Vec<String>]) {
    println!("{}", headers.join(","));
    for row in rows {
        let escaped: Vec<String> = row
            .iter()
            .map(|cell| {
                if cell.contains(',') || cell.contains('"') || cell.contains('\n') {
                    format!("\"{}\"", cell.replace('"', "\"\""))
                } else {
                    cell.clone()
                }
            })
            .collect();
        println!("{}", escaped.join(","));
    }
}

fn print_tsv(headers: &[&str], rows: &[Vec<String>]) {
    println!("{}", headers.join("\t"));
    for row in rows {
        println!("{}", row.join("\t"));
    }
}

fn print_json(headers: &[&str], rows: &[Vec<String>]) -> Result<()> {
    let json_rows: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            let mut obj = serde_json::Map::new();
            for (i, header) in headers.iter().enumerate() {
                let val = row.get(i).cloned().unwrap_or_default();
                obj.insert(header.to_string(), serde_json::Value::String(val));
            }
            serde_json::Value::Object(obj)
        })
        .collect();

    println!("{}", serde_json::to_string_pretty(&json_rows)?);
    Ok(())
}
