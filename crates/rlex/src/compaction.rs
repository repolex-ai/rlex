//! JSON-LD compaction for CONSTRUCT/DESCRIBE query results.
//!
//! Mirrors the Python lexq pattern:
//!   oxigraph serializes triples → expanded JSON-LD string
//!   json-ld parses + compacts using the embedded repolex context
//!   we return a compact JSON-LD string
//!
//! 85-90% token reduction target for LLM consumption.

use anyhow::{Context, Result};
use json_ld::{
    syntax::Parse, ExtractContext, JsonLdProcessor, Print, RemoteDocument, RemoteDocumentReference,
};

/// The embedded repolex JSON-LD context (~17KB, ~180 prefix mappings)
const REPOLEX_CONTEXT: &str = include_str!("../assets/repolex-context.jsonld");

/// Compact an expanded JSON-LD document (as a string) using the embedded repolex context.
///
/// Input: JSON-LD in expanded form (e.g. from `oxigraph::io::RdfFormat::JsonLd`)
/// Output: pretty-printed compacted JSON-LD with `@context` block applied
pub fn compact(expanded_jsonld: &str) -> Result<String> {
    // Parse the input expanded document into a json_syntax::Value
    let (input_value, _) = json_ld::syntax::Value::parse_str(expanded_jsonld)
        .map_err(|e| anyhow::anyhow!("parsing expanded JSON-LD: {}", e))?;
    let input_doc = RemoteDocumentReference::Loaded(RemoteDocument::new(None, None, input_value));

    // Parse the embedded context file (shape: {"@context": {...}}) and extract the Context
    let (ctx_value, _) = json_ld::syntax::Value::parse_str(REPOLEX_CONTEXT)
        .map_err(|e| anyhow::anyhow!("parsing embedded repolex context: {}", e))?;
    let ld_context = ctx_value
        .into_ld_context()
        .map_err(|e| anyhow::anyhow!("extracting @context from embedded file: {}", e))?;
    let ctx_ref = RemoteDocumentReference::Loaded(RemoteDocument::new(None, None, ld_context));

    // json-ld's compact is async — block on it via a fresh tokio runtime
    let rt = tokio::runtime::Runtime::new()
        .context("creating tokio runtime for JSON-LD compaction")?;

    let compacted = rt
        .block_on(async {
            let loader = json_ld::NoLoader;
            input_doc.compact(ctx_ref, &loader).await
        })
        .map_err(|e| anyhow::anyhow!("JSON-LD compaction failed: {}", e))?;

    Ok(compacted.pretty_print().to_string())
}
