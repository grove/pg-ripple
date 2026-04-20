//! RDF fixture loader — loads test data into pg_ripple via SQL.
//!
//! Each test case runs inside a PostgreSQL transaction that is rolled back
//! after the test completes, giving perfect isolation at zero cleanup cost.

use std::path::Path;

use postgres::Transaction;

/// Load an RDF file into the pg_ripple default graph (graph ID 0).
///
/// The file content is read by the test process and passed as a TEXT parameter
/// to the appropriate loader function based on file extension.
pub fn load_default_graph(
    tx: &mut Transaction<'_>,
    file: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let content =
        std::fs::read_to_string(file).map_err(|e| format!("reading {}: {e}", file.display()))?;
    match format_from_path(file) {
        "rdfxml" => {
            let content = inject_rdfxml_base(&content, file);
            tx.execute("SELECT pg_ripple.load_rdfxml($1, false)", &[&content])?
        }
        _ => {
            let content = inject_turtle_base(&content, file);
            tx.execute("SELECT pg_ripple.load_turtle($1, false)", &[&content])?
        }
    };
    Ok(())
}

/// Load an RDF file into a specific named graph in pg_ripple.
pub fn load_named_graph(
    tx: &mut Transaction<'_>,
    graph_iri: &str,
    file: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let content =
        std::fs::read_to_string(file).map_err(|e| format!("reading {}: {e}", file.display()))?;
    match format_from_path(file) {
        "rdfxml" => {
            let content = inject_rdfxml_base(&content, file);
            tx.execute(
                "SELECT pg_ripple.load_rdfxml_into_graph($1, $2)",
                &[&content, &graph_iri],
            )?
        }
        _ => {
            let content = inject_turtle_base(&content, file);
            tx.execute(
                "SELECT pg_ripple.load_turtle_into_graph($1, $2)",
                &[&content, &graph_iri],
            )?
        }
    };
    Ok(())
}

/// Inject an `xml:base` attribute into an RDF/XML document if none is present.
///
/// The W3C RDF/XML test files may contain relative IRIs (e.g. `rdf:resource=""`).
/// The `load_rdfxml` function requires an absolute base URI to resolve them.
/// This helper injects `xml:base="file:///..."` into the root `<rdf:RDF>` element
/// so the parser can resolve relative references correctly.
fn inject_rdfxml_base(content: &str, file: &Path) -> String {
    // If already has xml:base, leave as-is.
    if content.contains("xml:base") {
        return content.to_owned();
    }
    let base = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    let base_uri = format!("file://{}", base.display());
    // Inject xml:base before the closing '>' of the first XML element tag.
    if let Some(rdf_pos) = content.find("<rdf:RDF") {
        let after_tag = &content[rdf_pos..];
        if let Some(close) = after_tag.find('>') {
            let insert_pos = rdf_pos + close;
            let mut result = content.to_owned();
            result.insert_str(insert_pos, &format!(r#" xml:base="{base_uri}""#));
            return result;
        }
    }
    content.to_owned()
}

/// Inject a `@base` declaration into a Turtle document if none is present.
///
/// Turtle test files may use relative IRIs like `<>` or `<#local>`.
/// Without a base, the pg_ripple parser cannot resolve them.  This helper
/// prepends `@base <file:///absolute/path/to/file.ttl>` if no `@base` or
/// `BASE` is already present.
fn inject_turtle_base(content: &str, file: &Path) -> String {
    let trimmed = content.trim_start();
    // If the file already declares a base, leave it as-is.
    if trimmed.starts_with("@base") || trimmed.to_uppercase().starts_with("BASE") {
        return content.to_owned();
    }
    let base = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    let base_uri = format!("file://{}", base.display());
    format!("@base <{base_uri}> .\n{content}")
}

/// Detect the RDF format of a file from its extension.
pub fn format_from_path(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("ttl") | Some("n3") => "turtle",
        Some("nt") => "ntriples",
        Some("rdf") | Some("xml") => "rdfxml",
        Some("trig") => "trig",
        Some("nq") => "nquads",
        _ => "turtle",
    }
}

/// Load all fixtures for a test case (default graph + named graphs).
///
/// Only Turtle (`.ttl`, `.n3`) files are loaded via `load_turtle`.
/// RDF/XML (`.rdf`) files are loaded via `load_rdfxml` if available, or skipped.
/// Returns an error if any file cannot be read or the SQL call fails.
pub fn load_fixtures(
    tx: &mut Transaction<'_>,
    data_files: &[std::path::PathBuf],
    named_graphs: &[(String, std::path::PathBuf)],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Clear all graphs so pre-existing database triples don't bleed into the
    // test.  SPARQL queries without a FROM clause see all graphs, so we must
    // clear named graphs too — not just the default graph (g=0).  The enclosing
    // transaction is rolled back after each test, so this clear is automatically
    // undone — it's purely in-transaction.
    tx.execute("SELECT pg_ripple.sparql_update('CLEAR ALL')", &[])?;
    // Flush the in-memory encode cache so that stale term→id mappings from
    // previous rolled-back test transactions do not bleed into this test.
    // (The xact-callback fix for this is in v0.42.0; this is the workaround.)
    tx.execute("SELECT pg_ripple.flush_encode_cache()", &[])?;
    // Reset the SPARQL plan cache so that compiled SQL (which embeds dictionary
    // IDs) from a previous test is not reused for a different test where the
    // same query text maps to different predicate IDs after encode cache flush.
    tx.execute("SELECT pg_ripple.plan_cache_reset()", &[])?;
    for file in data_files {
        load_default_graph(tx, file)?;
    }
    for (iri, file) in named_graphs {
        load_named_graph(tx, iri, file)?;
    }
    Ok(())
}

/// Load SERVICE mock data for federation tests (v0.42.0).
///
/// For each `(endpoint_url, data_file)` pair:
/// 1. Load the RDF data file into a named graph whose IRI is the endpoint URL.
/// 2. Register the endpoint URL in `_pg_ripple.federation_endpoints` with
///    `graph_iri = endpoint_url`, so SERVICE clauses are rewritten to query
///    the local named graph instead of making HTTP calls.
///
/// All changes occur within the caller's transaction and are rolled back
/// automatically at the end of each test case.
pub fn load_service_data(
    tx: &mut Transaction<'_>,
    service_data: &[(String, std::path::PathBuf)],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for (endpoint_url, data_file) in service_data {
        // Load the endpoint's data into a named graph identified by the endpoint URL.
        load_named_graph(tx, endpoint_url, data_file)?;
        // Register the endpoint with graph_iri pointing to the named graph.
        tx.execute(
            "INSERT INTO _pg_ripple.federation_endpoints (url, enabled, graph_iri)
             VALUES ($1, true, $1)
             ON CONFLICT (url) DO UPDATE SET enabled = true, graph_iri = $1",
            &[endpoint_url],
        )?;
    }
    Ok(())
}
