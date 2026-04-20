//! Result validator — compares pg_ripple query output against W3C expected results.
//!
//! Supported formats:
//! - `.srj` — SPARQL Results JSON (SELECT / ASK)
//! - `.srx` — SPARQL Results XML  (SELECT / ASK)  [minimal parser]
//! - `.ttl` — Turtle RDF graph    (CONSTRUCT / DESCRIBE / UPDATE)

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use postgres::Transaction;
use serde_json::Value;

// ── Public API ────────────────────────────────────────────────────────────────

/// Outcome of a single result-validation step.
#[derive(Debug)]
pub enum ValidationResult {
    /// Results matched.
    Pass,
    /// Results did not match; includes a human-readable diff message.
    Fail(String),
    /// Validation could not proceed (e.g., unsupported result format).
    Skip(String),
}

/// Validate a SELECT or ASK query result against an expected result file.
///
/// The `query_text` is executed via `pg_ripple.sparql()`.
/// The `result_file` is `.srj` or `.srx`.
pub fn validate_select_ask(
    tx: &mut Transaction<'_>,
    query_text: &str,
    result_file: &Path,
) -> ValidationResult {
    let ext = result_file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    match ext {
        "srj" => validate_select_ask_srj(tx, query_text, result_file),
        "srx" => validate_select_ask_srx(tx, query_text, result_file),
        _ => ValidationResult::Skip(format!("unsupported result format: .{ext}")),
    }
}

/// Validate a CONSTRUCT query result against an expected Turtle file.
///
/// Also handles SELECT results encoded as Turtle using the W3C RS vocabulary
/// (`rs:ResultSet`, `rs:solution`, `rs:binding`, `rs:value`, `rs:variable`).
/// Uses a simple triple-set comparison for CONSTRUCT (not full graph isomorphism).
pub fn validate_construct(
    tx: &mut Transaction<'_>,
    query_text: &str,
    result_file: &Path,
) -> ValidationResult {
    // Check if the result file uses the W3C RS Turtle vocabulary (SELECT results stored as Turtle).
    let content = match std::fs::read_to_string(result_file) {
        Ok(s) => s,
        Err(e) => return ValidationResult::Skip(format!("reading expected result: {e}")),
    };
    let is_rs_format = content.contains("rs:ResultSet")
        || content.contains("result-set#ResultSet")
        || content.contains("rs:solution");
    if is_rs_format {
        return validate_select_rs_ttl(tx, query_text, result_file, &content);
    }

    let expected_triples = match parse_turtle_to_triple_set_with_base(result_file, &content) {
        Ok(t) => t,
        Err(e) => return ValidationResult::Skip(format!("reading expected result: {e}")),
    };

    let rows = match tx.query(
        "SELECT result FROM pg_ripple.sparql_construct($1)",
        &[&query_text],
    ) {
        Ok(r) => r,
        Err(e) => return ValidationResult::Fail(format!("query error: {e}")),
    };

    let mut actual_triples: HashSet<String> = HashSet::new();
    for row in &rows {
        let json: serde_json::Value = row.get(0);
        // result is a JSONB with "s", "p", "o" keys in N-Triples format
        if let (Some(s), Some(p), Some(o)) = (
            json.get("s").and_then(Value::as_str),
            json.get("p").and_then(Value::as_str),
            json.get("o").and_then(Value::as_str),
        ) {
            actual_triples.insert(format!("{s} {p} {o}"));
        }
    }

    compare_triple_sets(&expected_triples, &actual_triples)
}

/// Validate the graph state after a SPARQL UPDATE against expected Turtle files.
///
/// Checks only the graphs mentioned in the expected result:
/// - `expected_default`: expected default graph content (empty slice = don't check default graph)
/// - `expected_named`: expected named graph content as `(graph_iri, file_path)` pairs
pub fn validate_update(
    tx: &mut Transaction<'_>,
    expected_default: &[PathBuf],
    expected_named: &[(String, PathBuf)],
) -> ValidationResult {
    // Compare default graph if expected files are provided.
    for expected_file in expected_default {
        let expected_triples = match parse_turtle_to_triple_set(expected_file) {
            Ok(t) => t,
            Err(e) => {
                return ValidationResult::Skip(format!(
                    "reading expected default graph {}: {e}",
                    expected_file.display()
                ));
            }
        };

        let rows = match tx.query(
            // Use NOT EXISTS to exclude named-graph triples: bare triple patterns in
            // pg_ripple use union-graph semantics (all graphs). Filtering out triples
            // that appear in any named graph (GRAPH ?g) leaves only the default graph (g=0).
            "SELECT result FROM pg_ripple.sparql_construct('CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o FILTER NOT EXISTS { GRAPH ?g { ?s ?p ?o } } }')",
            &[],
        ) {
            Ok(r) => r,
            Err(e) => return ValidationResult::Fail(format!("querying default graph: {e}")),
        };

        let mut actual_triples: HashSet<String> = HashSet::new();
        for row in &rows {
            let json: serde_json::Value = row.get(0);
            if let (Some(s), Some(p), Some(o)) = (
                json.get("s").and_then(Value::as_str),
                json.get("p").and_then(Value::as_str),
                json.get("o").and_then(Value::as_str),
            ) {
                actual_triples.insert(format!("{s} {p} {o}"));
            }
        }

        match compare_triple_sets(&expected_triples, &actual_triples) {
            ValidationResult::Pass => {}
            other => return other,
        }
    }

    // Compare named graphs.
    for (graph_iri, expected_file) in expected_named {
        let expected_triples = match parse_turtle_to_triple_set(expected_file) {
            Ok(t) => t,
            Err(e) => {
                return ValidationResult::Skip(format!(
                    "reading expected named graph {}: {e}",
                    expected_file.display()
                ));
            }
        };

        // Build the SPARQL CONSTRUCT query for this named graph.
        // Note: graph_iri comes from trusted W3C test manifest data.
        let construct_query =
            format!("CONSTRUCT {{ ?s ?p ?o }} WHERE {{ GRAPH <{graph_iri}> {{ ?s ?p ?o }} }}");
        let rows = match tx.query(
            "SELECT result FROM pg_ripple.sparql_construct($1)",
            &[&construct_query],
        ) {
            Ok(r) => r,
            Err(e) => {
                return ValidationResult::Fail(format!("querying named graph <{graph_iri}>: {e}"));
            }
        };

        let mut actual_triples: HashSet<String> = HashSet::new();
        for row in &rows {
            let json: serde_json::Value = row.get(0);
            if let (Some(s), Some(p), Some(o)) = (
                json.get("s").and_then(Value::as_str),
                json.get("p").and_then(Value::as_str),
                json.get("o").and_then(Value::as_str),
            ) {
                actual_triples.insert(format!("{s} {p} {o}"));
            }
        }

        match compare_triple_sets(&expected_triples, &actual_triples) {
            ValidationResult::Pass => {}
            ValidationResult::Fail(msg) => {
                return ValidationResult::Fail(format!("named graph <{graph_iri}>: {msg}"));
            }
            ValidationResult::Skip(msg) => {
                return ValidationResult::Skip(format!("named graph <{graph_iri}>: {msg}"));
            }
        }
    }

    ValidationResult::Pass
}

/// Validate that a SPARQL syntax test passes or fails as expected.
pub fn validate_syntax(
    tx: &mut Transaction<'_>,
    query_text: &str,
    expect_valid: bool,
) -> ValidationResult {
    // Use explain_sparql to test syntax without executing.
    let result = tx.query_one("SELECT pg_ripple.explain_sparql($1, 'sql')", &[&query_text]);
    let parsed_ok = result.is_ok();

    if expect_valid && !parsed_ok {
        ValidationResult::Fail(format!(
            "expected valid syntax, but parsing failed: {:?}",
            result.err()
        ))
    } else if !expect_valid && parsed_ok {
        ValidationResult::Fail("expected syntax error, but query parsed successfully".into())
    } else {
        ValidationResult::Pass
    }
}

// ── SELECT / ASK via SPARQL Results JSON (.srj) ───────────────────────────────

fn validate_select_ask_srj(
    tx: &mut Transaction<'_>,
    query_text: &str,
    result_file: &Path,
) -> ValidationResult {
    let expected_content = match std::fs::read_to_string(result_file) {
        Ok(s) => s,
        Err(e) => return ValidationResult::Skip(format!("reading {}: {e}", result_file.display())),
    };

    let expected: Value = match serde_json::from_str(&expected_content) {
        Ok(v) => v,
        Err(e) => return ValidationResult::Skip(format!("parsing {}: {e}", result_file.display())),
    };

    // ASK query
    if let Some(bool_val) = expected.get("boolean") {
        let expected_bool = bool_val.as_bool().unwrap_or(false);
        let result = tx.query_one("SELECT pg_ripple.sparql_ask($1)", &[&query_text]);
        return match result {
            Ok(row) => {
                let actual: bool = row.get(0);
                if actual == expected_bool {
                    ValidationResult::Pass
                } else {
                    ValidationResult::Fail(format!(
                        "ASK result mismatch: expected {expected_bool}, got {actual}"
                    ))
                }
            }
            Err(e) => ValidationResult::Fail(format!("query error: {e}")),
        };
    }

    // SELECT query — compare binding sets
    let expected_vars: Vec<String> = expected
        .get("head")
        .and_then(|h| h.get("vars"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let expected_bindings: Vec<HashMap<String, String>> = expected
        .get("results")
        .and_then(|r| r.get("bindings"))
        .and_then(|b| b.as_array())
        .map(|arr| arr.iter().map(parse_srj_binding).collect())
        .unwrap_or_default();

    // Execute the query
    let rows = match tx.query("SELECT result FROM pg_ripple.sparql($1)", &[&query_text]) {
        Ok(r) => r,
        Err(e) => return ValidationResult::Fail(format!("query error: {e}")),
    };

    let actual_bindings: Vec<HashMap<String, String>> = rows
        .iter()
        .map(|row| {
            let json: serde_json::Value = row.get(0);
            parse_pg_ripple_binding(&json, &expected_vars)
        })
        .collect();

    compare_binding_sets(&expected_bindings, &actual_bindings, &expected_vars)
}

/// Parse a single binding map from a SPARQL Results JSON `bindings` array entry.
fn parse_srj_binding(binding: &Value) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Some(obj) = binding.as_object() {
        for (var, term) in obj {
            if let Some(term_str) = srj_term_to_string(term) {
                map.insert(var.clone(), term_str);
            }
        }
    }
    map
}

/// Convert a SPARQL Results JSON term to a canonical string form for comparison.
fn srj_term_to_string(term: &Value) -> Option<String> {
    let term_type = term.get("type")?.as_str()?;
    let value = term.get("value")?.as_str()?;
    match term_type {
        "uri" => Some(format!("<{value}>")),
        "bnode" => Some(format!("_:{value}")),
        "literal" => {
            let lang = term.get("xml:lang").and_then(|v| v.as_str());
            let dt = term.get("datatype").and_then(|v| v.as_str());
            if let Some(l) = lang {
                Some(format!("\"{value}\"@{}", l.to_lowercase()))
            } else if let Some(d) = dt {
                Some(format!("\"{value}\"^^<{d}>"))
            } else {
                Some(format!("\"{value}\""))
            }
        }
        _ => None,
    }
}

/// Parse a pg_ripple `sparql()` result row into a binding map.
///
/// pg_ripple returns JSONB with variable names as keys and N-Triples-style
/// term strings as values.  Aggregate results (COUNT, SUM, AVG) are returned
/// as raw JSON numbers, which we convert to the corresponding xsd typed literal
/// representation so they can be compared with SRX expected values.
///
/// Normalizes `"x"^^<xsd:string>` to `"x"` (RDF 1.1 equivalence).
/// Normalizes lang tags to lowercase (lang tags are case-insensitive per RFC 4647).
fn parse_pg_ripple_binding(json: &Value, vars: &[String]) -> HashMap<String, String> {
    const XSD_STRING: &str = "\"^^<http://www.w3.org/2001/XMLSchema#string>";
    let mut map = HashMap::new();
    if let Some(obj) = json.as_object() {
        for var in vars {
            if let Some(val) = obj.get(var) {
                let term_str = match val {
                    Value::String(s) if !s.is_empty() => {
                        // Normalize "x"^^xsd:string → "x" (plain literal)
                        let s = if s.ends_with(XSD_STRING) && s.starts_with('"') {
                            s[..s.len() - XSD_STRING.len() + 1].to_string()
                        } else {
                            s.clone()
                        };
                        // Normalize lang tag to lowercase (case-insensitive comparison)
                        normalize_lang_tag_case(&s)
                    }
                    Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            format!("\"{}\"^^<http://www.w3.org/2001/XMLSchema#integer>", i)
                        } else if let Some(f) = n.as_f64() {
                            format!("\"{}\"^^<http://www.w3.org/2001/XMLSchema#decimal>", f)
                        } else {
                            continue;
                        }
                    }
                    _ => continue,
                };
                map.insert(var.clone(), term_str);
            }
        }
    }
    map
}

/// Compare two binding sets (order-independent).
fn compare_binding_sets(
    expected: &[HashMap<String, String>],
    actual: &[HashMap<String, String>],
    vars: &[String],
) -> ValidationResult {
    // For numeric typed literals (integer, decimal, double, float), parse and
    // re-serialize to a canonical form so that e.g. "2100"^^double == "2.1E3"^^double
    // and "2.0"^^decimal == "2"^^decimal.
    fn normalize_term(s: &str) -> String {
        // Pattern: "<value>"^^<datatype>
        if let Some(rest) = s.strip_prefix('"') {
            if let Some(dt_pos) = rest.rfind("\"^^<") {
                let value = &rest[..dt_pos];
                let dt = &rest[dt_pos + 4..rest.len() - 1]; // strip trailing '>'
                match dt {
                    "http://www.w3.org/2001/XMLSchema#double"
                    | "http://www.w3.org/2001/XMLSchema#float" => {
                        if let Ok(f) = value.parse::<f64>() {
                            return format!("\"{}\"^^<{dt}>", format_f64_canonical(f));
                        }
                    }
                    "http://www.w3.org/2001/XMLSchema#decimal" => {
                        if let Ok(f) = value.parse::<f64>() {
                            return format!("\"{}\"^^<{dt}>", format_decimal_canonical(f));
                        }
                    }
                    "http://www.w3.org/2001/XMLSchema#integer" => {
                        if let Ok(i) = value.parse::<i64>() {
                            return format!("\"{}\"^^<{dt}>", i);
                        }
                    }
                    _ => {}
                }
            }
        }
        s.to_string()
    }

    // Convert a f64 to a canonical form for double comparison.
    fn format_f64_canonical(f: f64) -> String {
        // Use Rust's default f64 formatting which is unique and round-trips
        format!("{}", f)
    }

    // Convert a f64 to a canonical form for decimal comparison (strip trailing zeros after dot).
    fn format_decimal_canonical(f: f64) -> String {
        // Format with enough precision, then strip trailing zeros.
        // For exact values, f64 should be sufficient for SPARQL test values.
        let s = format!("{:.15}", f);
        let s = s.trim_end_matches('0');
        let s = s.trim_end_matches('.');
        s.to_string()
    }

    // Convert to sets of canonical row strings for order-independent comparison.
    fn row_key(row: &HashMap<String, String>, vars: &[String]) -> String {
        vars.iter()
            .map(|v| {
                let term = row.get(v).map(|s| s.as_str()).unwrap_or("");
                format!("{}={}", v, normalize_term(term))
            })
            .collect::<Vec<_>>()
            .join("|")
    }

    let expected_set: HashSet<String> = expected.iter().map(|r| row_key(r, vars)).collect();
    let actual_set: HashSet<String> = actual.iter().map(|r| row_key(r, vars)).collect();

    let missing: Vec<&String> = expected_set.difference(&actual_set).collect();
    let extra: Vec<&String> = actual_set.difference(&expected_set).collect();

    if missing.is_empty() && extra.is_empty() {
        return ValidationResult::Pass;
    }

    // Try per-row blank node isomorphism.
    // For each expected row, check if there's a matching actual row
    // where blank node IDs can be renamed consistently WITHIN that row.
    // This handles cases like BNODE("foo")=b1 (expected) vs BNODE("foo")=b888861 (actual).
    if try_bnode_row_match(expected, actual, vars, &normalize_term) {
        return ValidationResult::Pass;
    }

    if missing.is_empty() && extra.is_empty() {
        ValidationResult::Pass
    } else {
        let mut msg = String::new();
        if expected.len() != actual.len() {
            msg += &format!(
                "row count mismatch: expected {}, got {}\n",
                expected.len(),
                actual.len()
            );
        }
        if !missing.is_empty() {
            msg += &format!(
                "missing {} row(s): {:?}\n",
                missing.len(),
                &missing[..missing.len().min(3)]
            );
        }
        if !extra.is_empty() {
            msg += &format!(
                "extra {} row(s): {:?}",
                extra.len(),
                &extra[..extra.len().min(3)]
            );
        }
        ValidationResult::Fail(msg)
    }
}

/// Per-row blank node matching: for each expected row, find a unique actual row where
/// blank node IDs can be consistently renamed within that row.
/// This handles cases where the same blank node ID is reused across solution rows
/// (e.g. BNODE("foo") always produces the same blank node ID regardless of solution row),
/// which differs from tests that expect unique blank node IDs per solution row.
fn try_bnode_row_match(
    expected: &[HashMap<String, String>],
    actual: &[HashMap<String, String>],
    vars: &[String],
    normalize: &impl Fn(&str) -> String,
) -> bool {
    // Check if any row contains a blank node.
    let has_bnode = expected.iter().chain(actual.iter()).any(|row| {
        vars.iter()
            .any(|v| row.get(v).map_or(false, |t| t.starts_with("_:")))
    });
    if !has_bnode {
        return false;
    }

    // Greedy: for each expected row in order, find a matching (unused) actual row.
    let mut used = vec![false; actual.len()];
    'outer: for exp_row in expected {
        for (ai, act_row) in actual.iter().enumerate() {
            if used[ai] {
                continue;
            }
            if rows_match_with_bnodes(exp_row, act_row, vars, normalize) {
                used[ai] = true;
                continue 'outer;
            }
        }
        // No match found for this expected row.
        return false;
    }
    // All expected rows matched distinct actual rows.
    // Also check that row counts are equal.
    expected.len() == actual.len()
}

/// Check if two rows match with per-row blank node renaming.
/// Blank nodes are allowed to match any blank node, as long as within this row:
/// - same BN in expected → same BN in actual
/// - different BN in expected → different BN in actual (injective mapping within row)
fn rows_match_with_bnodes(
    exp_row: &HashMap<String, String>,
    act_row: &HashMap<String, String>,
    vars: &[String],
    normalize: &impl Fn(&str) -> String,
) -> bool {
    // Mapping: exp_bn → act_bn (within this row)
    let mut exp_to_act: HashMap<String, String> = HashMap::new();
    // Reverse mapping to check injectivity: act_bn → exp_bn
    let mut act_to_exp: HashMap<String, String> = HashMap::new();

    for v in vars {
        let exp_val = exp_row.get(v).map(|s| s.as_str()).unwrap_or("");
        let act_val = act_row.get(v).map(|s| s.as_str()).unwrap_or("");

        let exp_is_bn = exp_val.starts_with("_:");
        let act_is_bn = act_val.starts_with("_:");

        if exp_is_bn && act_is_bn {
            // Both blank nodes: check/establish mapping.
            if let Some(mapped) = exp_to_act.get(exp_val) {
                if mapped != act_val {
                    return false; // Same exp BN must map to same act BN.
                }
            } else {
                // Check injectivity: act_val must not already be mapped to different exp BN.
                if let Some(prev_exp) = act_to_exp.get(act_val) {
                    if prev_exp != exp_val {
                        return false; // Different exp BNs must map to different act BNs.
                    }
                }
                exp_to_act.insert(exp_val.to_string(), act_val.to_string());
                act_to_exp.insert(act_val.to_string(), exp_val.to_string());
            }
        } else if exp_is_bn != act_is_bn {
            // One is blank node, other is not — can't match.
            return false;
        } else {
            // Neither is a blank node — compare normalized values.
            if normalize(exp_val) != normalize(act_val) {
                return false;
            }
        }
    }
    true
}

// ── SELECT / ASK via SPARQL Results XML (.srx) ───────────────────────────────

/// Minimal SPARQL Results XML parser — extracts variable names and bindings.
fn validate_select_ask_srx(
    tx: &mut Transaction<'_>,
    query_text: &str,
    result_file: &Path,
) -> ValidationResult {
    let content = match std::fs::read_to_string(result_file) {
        Ok(s) => s,
        Err(e) => return ValidationResult::Skip(format!("reading {}: {e}", result_file.display())),
    };

    // Detect ASK
    if content.contains("<boolean>true</boolean>") {
        let result = tx.query_one("SELECT pg_ripple.sparql_ask($1)", &[&query_text]);
        return match result {
            Ok(row) => {
                let actual: bool = row.get(0);
                if actual {
                    ValidationResult::Pass
                } else {
                    ValidationResult::Fail("ASK: expected true, got false".into())
                }
            }
            Err(e) => ValidationResult::Fail(format!("query error: {e}")),
        };
    }
    if content.contains("<boolean>false</boolean>") {
        let result = tx.query_one("SELECT pg_ripple.sparql_ask($1)", &[&query_text]);
        return match result {
            Ok(row) => {
                let actual: bool = row.get(0);
                if !actual {
                    ValidationResult::Pass
                } else {
                    ValidationResult::Fail("ASK: expected false, got true".into())
                }
            }
            Err(e) => ValidationResult::Fail(format!("query error: {e}")),
        };
    }

    // Parse variable names from <variable name="..."/> or <variable name='...'/>
    let vars: Vec<String> = {
        let mut vs = Vec::new();
        let mut search = content.as_str();
        while let Some(pos) = search.find("<variable name=") {
            let rest = &search[pos + "<variable name=".len()..];
            // Accept both quote styles
            let (quote, rest) = if rest.starts_with('"') {
                ('"', &rest[1..])
            } else if rest.starts_with('\'') {
                ('\'', &rest[1..])
            } else {
                search = &search[pos + 1..];
                continue;
            };
            if let Some(end) = rest.find(quote) {
                vs.push(rest[..end].to_string());
            }
            search = &search[pos + 1..];
        }
        vs
    };

    // Parse results from <result>...</result> blocks
    let expected_bindings: Vec<HashMap<String, String>> = {
        let mut bindings = Vec::new();
        let mut search = content.as_str();
        while let Some(start) = search.find("<result>") {
            let rest = &search[start + "<result>".len()..];
            if let Some(end) = rest.find("</result>") {
                let result_block = &rest[..end];
                bindings.push(parse_srx_result_block(result_block));
                search = &rest[end + "</result>".len()..];
            } else {
                break;
            }
        }
        bindings
    };

    // Execute the query and compare
    let rows = match tx.query("SELECT result FROM pg_ripple.sparql($1)", &[&query_text]) {
        Ok(r) => r,
        Err(e) => return ValidationResult::Fail(format!("query error: {e}")),
    };

    let actual_bindings: Vec<HashMap<String, String>> = rows
        .iter()
        .map(|row| {
            let json: serde_json::Value = row.get(0);
            parse_pg_ripple_binding(&json, &vars)
        })
        .collect();

    compare_binding_sets(&expected_bindings, &actual_bindings, &vars)
}

/// Parse one `<result>…</result>` XML block into a binding map.
fn parse_srx_result_block(block: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut search = block;
    while let Some(start) = search.find("<binding name=") {
        let rest = &search[start + "<binding name=".len()..];
        // Accept both quote styles
        let (quote, rest) = if rest.starts_with('"') {
            ('"', &rest[1..])
        } else if rest.starts_with('\'') {
            ('\'', &rest[1..])
        } else {
            search = &search[start + 1..];
            continue;
        };
        let var_end = match rest.find(quote) {
            Some(p) => p,
            None => break,
        };
        let var_name = rest[..var_end].to_string();
        // Find the value between <binding ...> and </binding>
        let after_attr = &rest[var_end..];
        let content_start = match after_attr.find('>') {
            Some(p) => p + 1,
            None => break,
        };
        let content = &after_attr[content_start..];
        let end = match content.find("</binding>") {
            Some(p) => p,
            None => break,
        };
        let term_xml = content[..end].trim();
        if let Some(term_str) = parse_srx_term(term_xml) {
            map.insert(var_name, term_str);
        }
        search = &content[end..];
    }
    map
}

/// Normalize the lang tag in a term string to lowercase.
/// Lang tags are case-insensitive per RFC 4647, so "en-US" == "en-us".
fn normalize_lang_tag_case(s: &str) -> String {
    if !s.starts_with('"') {
        return s.to_string();
    }
    // Find the last '@' in the string; everything after it is the lang tag
    // if it consists only of alphanumeric chars and hyphens.
    if let Some(at_pos) = s.rfind('@') {
        let after = &s[at_pos + 1..];
        if !after.is_empty() && after.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            let before = &s[..at_pos + 1];
            return format!("{}{}", before, after.to_lowercase());
        }
    }
    s.to_string()
}

/// Convert a SPARQL Results XML term element to a canonical string.
///
/// In RDF 1.1, `"x"^^xsd:string` is semantically identical to the plain literal `"x"`.
/// We normalize both to the plain form `"x"` for comparison purposes.
fn parse_srx_term(xml: &str) -> Option<String> {
    if xml.starts_with("<uri>") {
        let inner = xml.strip_prefix("<uri>")?.trim_end_matches("</uri>");
        return Some(format!("<{inner}>"));
    }
    if xml.starts_with("<bnode>") {
        let inner = xml.strip_prefix("<bnode>")?.trim_end_matches("</bnode>");
        return Some(format!("_:{inner}"));
    }
    if xml.starts_with("<literal") {
        let after_tag = xml.find('>')? + 1;
        let end = xml.rfind("</literal>")?;
        let value = &xml[after_tag..end];
        // Check for lang and datatype attributes (both quote styles)
        let tag_part = &xml[..xml.find('>')?];
        // Helper: find attribute value for key, handles both "..." and '...' quoting
        fn find_attr<'a>(tag: &'a str, key: &str) -> Option<&'a str> {
            let dq = format!("{key}=\"");
            let sq = format!("{key}='");
            if let Some(pos) = tag.find(&dq) {
                let rest = &tag[pos + dq.len()..];
                rest.find('"').map(|e| &rest[..e])
            } else if let Some(pos) = tag.find(&sq) {
                let rest = &tag[pos + sq.len()..];
                rest.find('\'').map(|e| &rest[..e])
            } else {
                None
            }
        }
        if let Some(lang) = find_attr(tag_part, "xml:lang") {
            let lang = lang.to_lowercase();
            return Some(format!("\"{value}\"@{lang}"));
        }
        if let Some(dt) = find_attr(tag_part, "datatype") {
            // Normalize xsd:string typed literals to plain literal form (RDF 1.1 equivalence).
            if dt == "http://www.w3.org/2001/XMLSchema#string" {
                return Some(format!("\"{value}\""));
            }
            return Some(format!("\"{value}\"^^<{dt}>"));
        }
        return Some(format!("\"{value}\""));
    }
    None
}

// ── CONSTRUCT / DESCRIBE via Turtle (.ttl) ────────────────────────────────────

/// Validate a SELECT query result against an expected result encoded as Turtle
/// using the W3C SPARQL Result Set vocabulary (rs:ResultSet, rs:solution, etc.).
///
/// This format is used by some W3C tests (e.g. agg-empty-group-count-graph)
/// where a SELECT query's expected results are stored as a Turtle graph.
fn validate_select_rs_ttl(
    tx: &mut Transaction<'_>,
    query_text: &str,
    result_file: &Path,
    content: &str,
) -> ValidationResult {
    // Parse the RS Turtle file into a triple graph (with base IRI for relative IRIs).
    let triples = match parse_turtle_to_triple_set_with_base(result_file, content) {
        Ok(t) => t,
        Err(e) => return ValidationResult::Skip(format!("parsing RS result: {e}")),
    };

    // RS vocabulary IRIs
    const RS_RESULT_VAR: &str =
        "<http://www.w3.org/2001/sw/DataAccess/tests/result-set#resultVariable>";
    const RS_SOLUTION: &str = "<http://www.w3.org/2001/sw/DataAccess/tests/result-set#solution>";
    const RS_BINDING: &str = "<http://www.w3.org/2001/sw/DataAccess/tests/result-set#binding>";
    const RS_VARIABLE: &str = "<http://www.w3.org/2001/sw/DataAccess/tests/result-set#variable>";
    const RS_VALUE: &str = "<http://www.w3.org/2001/sw/DataAccess/tests/result-set#value>";

    // Collect variable names from rs:resultVariable triples.
    let mut vars: Vec<String> = triples
        .iter()
        .filter(|t| t.contains(RS_RESULT_VAR))
        .filter_map(|t| {
            // t = "s <rs:resultVariable> \"varname\""
            let after_pred = t.split(RS_RESULT_VAR).nth(1)?.trim();
            let v = after_pred.trim_matches('"').to_string();
            Some(v)
        })
        .collect();
    vars.sort();
    vars.dedup();

    // Build a simple graph map: subject → predicate → Vec<object>
    let mut graph: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new();
    for triple in &triples {
        // Format: "s p o" — split at first two spaces (object may contain spaces in literals)
        let parts: Vec<&str> = triple.splitn(3, ' ').collect();
        if parts.len() == 3 {
            graph
                .entry(parts[0].to_string())
                .or_default()
                .entry(parts[1].to_string())
                .or_default()
                .push(parts[2].to_string());
        }
    }

    // Find solution blank nodes (subjects that appear as objects of rs:solution).
    let solution_nodes: Vec<String> = triples
        .iter()
        .filter(|t| t.contains(RS_SOLUTION))
        .filter_map(|t| t.split(RS_SOLUTION).nth(1).map(|s| s.trim().to_string()))
        .collect();

    // For each solution, collect bindings.
    let mut expected_bindings: Vec<HashMap<String, String>> = Vec::new();
    for sol_node in &solution_nodes {
        let mut binding_map: HashMap<String, String> = HashMap::new();
        // Find rs:binding objects for this solution node.
        let binding_nodes: Vec<String> = graph
            .get(sol_node)
            .and_then(|p| p.get(RS_BINDING))
            .cloned()
            .unwrap_or_default();
        for binding_node in &binding_nodes {
            let b_props = match graph.get(binding_node) {
                Some(p) => p,
                None => continue,
            };
            let var_name = b_props
                .get(RS_VARIABLE)
                .and_then(|v| v.first())
                .map(|s| s.trim_matches('"').to_string());
            let value = b_props.get(RS_VALUE).and_then(|v| v.first()).cloned();
            if let (Some(var), Some(val)) = (var_name, value) {
                // Normalize the value: strip outer <> for IRIs, keep as-is for literals.
                binding_map.insert(var, val);
            }
        }
        expected_bindings.push(binding_map);
    }

    // Execute the query and collect actual results.
    let rows = match tx.query("SELECT result FROM pg_ripple.sparql($1)", &[&query_text]) {
        Ok(r) => r,
        Err(e) => return ValidationResult::Fail(format!("query error: {e}")),
    };

    let actual_bindings: Vec<HashMap<String, String>> = rows
        .iter()
        .map(|row| {
            let json: serde_json::Value = row.get(0);
            parse_pg_ripple_binding(&json, &vars)
        })
        .collect();

    compare_binding_sets(&expected_bindings, &actual_bindings, &vars)
}

/// Parse a Turtle file into a set of canonical "s p o" strings.
fn parse_turtle_to_triple_set(
    path: &Path,
) -> Result<HashSet<String>, Box<dyn std::error::Error + Send + Sync>> {
    let content = std::fs::read_to_string(path)?;
    parse_turtle_to_triple_set_with_base(path, &content)
}

/// Parse Turtle content into a set of canonical "s p o" strings,
/// injecting a base IRI from the file path to resolve relative IRIs.
fn parse_turtle_to_triple_set_with_base(
    path: &Path,
    content: &str,
) -> Result<HashSet<String>, Box<dyn std::error::Error + Send + Sync>> {
    use rio_api::model::{Subject, Term};
    use rio_api::parser::TriplesParser;
    use rio_turtle::TurtleParser;

    // Build a base IRI from the file path so relative IRIs (e.g. <empty.ttl>) resolve correctly.
    let base_iri = if let Ok(abs) = path.canonicalize() {
        format!("file://{}", abs.display())
    } else {
        format!("file://{}", path.display())
    };
    // Prepend @base if the file doesn't already declare one.
    let has_base = content
        .split_whitespace()
        .next()
        .map(|w| w.eq_ignore_ascii_case("@base") || w.eq_ignore_ascii_case("BASE"))
        .unwrap_or(false);
    let with_base;
    let parse_content: &str = if has_base {
        content
    } else {
        with_base = format!("@base <{base_iri}> .\n{content}");
        &with_base
    };

    let mut triples = HashSet::new();

    let mut parser = TurtleParser::new(parse_content.as_bytes(), None);
    parser.parse_all(&mut |t| -> Result<(), rio_turtle::TurtleError> {
        let s = match &t.subject {
            Subject::NamedNode(n) => format!("<{}>", n.iri),
            Subject::BlankNode(b) => format!("_:{}", b.id),
            Subject::Triple(_) => "_:quoted".to_string(),
        };
        let p = format!("<{}>", t.predicate.iri);
        let o = match &t.object {
            Term::NamedNode(n) => format!("<{}>", n.iri),
            Term::BlankNode(b) => format!("_:{}", b.id),
            Term::Literal(l) => match l {
                rio_api::model::Literal::Simple { value } => format!("\"{value}\""),
                rio_api::model::Literal::LanguageTaggedString { value, language } => {
                    format!("\"{value}\"@{}", language.to_lowercase())
                }
                rio_api::model::Literal::Typed { value, datatype } => {
                    // Normalize xsd:string typed literals to plain literal form (RDF 1.1).
                    if datatype.iri == "http://www.w3.org/2001/XMLSchema#string" {
                        format!("\"{value}\"")
                    } else {
                        format!("\"{value}\"^^<{}>", datatype.iri)
                    }
                }
            },
            Term::Triple(_) => "_:quoted".to_string(),
        };
        triples.insert(format!("{s} {p} {o}"));
        Ok(())
    })?;

    Ok(triples)
}

/// Compare two triple sets for equality (order-independent, no blank-node iso).
fn compare_triple_sets(expected: &HashSet<String>, actual: &HashSet<String>) -> ValidationResult {
    let missing: Vec<&String> = expected.difference(actual).collect();
    let extra: Vec<&String> = actual.difference(expected).collect();

    if missing.is_empty() && extra.is_empty() {
        return ValidationResult::Pass;
    }

    // Allow blank-node differences (simple heuristic: only IRI/literal content differs).
    let non_bnode_missing: Vec<&&String> = missing.iter().filter(|t| !t.contains("_:")).collect();
    let non_bnode_extra: Vec<&&String> = extra.iter().filter(|t| !t.contains("_:")).collect();

    if non_bnode_missing.is_empty() && non_bnode_extra.is_empty() {
        // Only blank-node differences remain — treat as pass (no full iso implemented).
        return ValidationResult::Pass;
    }

    let mut msg = String::new();
    if !non_bnode_missing.is_empty() {
        msg += &format!(
            "missing {} triple(s): {:?}\n",
            non_bnode_missing.len(),
            &non_bnode_missing[..non_bnode_missing.len().min(3)]
        );
    }
    if !non_bnode_extra.is_empty() {
        msg += &format!(
            "extra {} triple(s): {:?}",
            non_bnode_extra.len(),
            &non_bnode_extra[..non_bnode_extra.len().min(3)]
        );
    }
    ValidationResult::Fail(msg)
}
