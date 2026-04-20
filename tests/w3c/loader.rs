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
        _ => tx.execute("SELECT pg_ripple.load_turtle($1, false)", &[&content])?,
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
        _ => tx.execute(
            "SELECT pg_ripple.load_turtle_into_graph($1, $2)",
            &[&content, &graph_iri],
        )?,
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
    let base = file
        .canonicalize()
        .unwrap_or_else(|_| file.to_path_buf());
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
    for file in data_files {
        load_default_graph(tx, file)?;
    }
    for (iri, file) in named_graphs {
        load_named_graph(tx, iri, file)?;
    }
    Ok(())
}
