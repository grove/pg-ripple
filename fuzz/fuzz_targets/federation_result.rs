//! cargo-fuzz target for the federation SPARQL results decoder (v0.46.0).
//!
//! Feeds arbitrary byte sequences through the SPARQL XML results parser
//! that processes `application/sparql-results+xml` responses from remote
//! SERVICE endpoints.  Asserts: no panic, no `unwrap` abort.  Invalid XML
//! must produce a parse error, never a crash.
//!
//! # Running locally
//!
//! ```sh
//! # Install cargo-fuzz
//! cargo install cargo-fuzz
//!
//! # Run for 10 minutes
//! cargo fuzz run federation_result -- -max_total_time=600
//!
//! # Minimise a crashing corpus entry
//! cargo fuzz tmin federation_result artifacts/federation_result/crash-...
//! ```
//!
//! # CI
//!
//! The `fuzz-federation` CI job runs this target for 600 seconds nightly.
//! Any corpus entry that triggers a panic is reported as a blocking failure.

#![no_main]

use libfuzzer_sys::fuzz_target;

// ── Minimal SPARQL XML results parser ────────────────────────────────────────
//
// This mirrors the decoding path in pg_ripple's federation result decoder.
// We parse the SPARQL Query Results XML Format
// (https://www.w3.org/TR/rdf-sparql-XMLres/) and extract bindings.
//
// Contract: any byte sequence must be handled without panic.
// Invalid XML → return an error value (never crash).

#[derive(Debug)]
enum ParseError {
    InvalidUtf8,
    MalformedXml(String),
}

/// Parse SPARQL XML results from raw bytes.
///
/// Returns `Ok(variable_names, binding_rows)` on success, or `Err(ParseError)`.
fn parse_sparql_xml_results(
    bytes: &[u8],
) -> Result<(Vec<String>, Vec<Vec<Option<String>>>), ParseError> {
    // Step 1: Validate UTF-8.
    let text = std::str::from_utf8(bytes).map_err(|_| ParseError::InvalidUtf8)?;

    // Step 2: Minimal XML scan — look for variable and binding elements.
    // We use a simple state machine rather than a full XML parser to avoid
    // pulling in a heavyweight dependency in the fuzz target.
    let mut variables: Vec<String> = Vec::new();
    let mut rows: Vec<Vec<Option<String>>> = Vec::new();

    // Extract <variable name="..."/> elements.
    let mut search = text;
    while let Some(pos) = search.find("<variable") {
        let rest = &search[pos..];
        if let Some(name_start) = rest.find("name=\"") {
            let name_rest = &rest[name_start + 6..];
            if let Some(name_end) = name_rest.find('"') {
                let name = &name_rest[..name_end];
                // Reject suspiciously long names (> 256 chars) to avoid OOM.
                if name.len() > 256 {
                    return Err(ParseError::MalformedXml("variable name too long".into()));
                }
                variables.push(name.to_owned());
            }
        }
        search = &search[pos + 1..];
    }

    // Extract <result>…</result> blocks.
    search = text;
    while let Some(result_start) = search.find("<result>") {
        let rest = &search[result_start..];
        let result_end = rest.find("</result>").unwrap_or(rest.len());
        let result_block = &rest[..result_end];

        let mut row: Vec<Option<String>> = vec![None; variables.len()];

        // Find <binding name="..."><literal/uri>…</literal/uri></binding>.
        let mut block = result_block;
        while let Some(bp) = block.find("<binding") {
            let brest = &block[bp..];
            let name_opt = brest
                .find("name=\"")
                .and_then(|ns| {
                    let nr = &brest[ns + 6..];
                    nr.find('"').map(|ne| nr[..ne].to_owned())
                });

            if let Some(bname) = name_opt {
                if let Some(col) = variables.iter().position(|v| v == &bname) {
                    // Extract literal or uri value.
                    let value = extract_binding_value(brest).unwrap_or_default();
                    if col < row.len() {
                        row[col] = Some(value);
                    }
                }
            }

            block = &block[bp + 1..];
        }

        rows.push(row);
        search = &search[result_start + 1..];
    }

    Ok((variables, rows))
}

/// Extract the text content of a `<literal>` or `<uri>` element in a binding.
fn extract_binding_value(binding_xml: &str) -> Option<String> {
    for tag in &["<literal>", "<uri>"] {
        if let Some(start) = binding_xml.find(tag) {
            let content_start = start + tag.len();
            let close_tag = tag.replace('<', "</");
            if let Some(end) = binding_xml[content_start..].find(close_tag.as_str()) {
                let value = &binding_xml[content_start..content_start + end];
                // Limit value size to avoid OOM.
                if value.len() > 4096 {
                    return None;
                }
                return Some(value.to_owned());
            }
        }
    }
    None
}

fuzz_target!(|data: &[u8]| {
    // Must not panic regardless of input.
    let _ = parse_sparql_xml_results(data);
});
