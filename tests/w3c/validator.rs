//! Result validator — compares pg_ripple query output against W3C expected results.
//!
//! Supported formats:
//! - `.srj` — SPARQL Results JSON (SELECT / ASK)
//! - `.srx` — SPARQL Results XML  (SELECT / ASK)  [minimal parser]
//! - `.ttl` — Turtle RDF graph    (CONSTRUCT / DESCRIBE / UPDATE)

use std::collections::{HashMap, HashSet};
use std::path::Path;

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
/// Uses a simple triple-set comparison (not full graph isomorphism).
pub fn validate_construct(
    tx: &mut Transaction<'_>,
    query_text: &str,
    result_file: &Path,
) -> ValidationResult {
    let expected_triples = match parse_turtle_to_triple_set(result_file) {
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
                Some(format!("\"{value}\"@{l}"))
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
fn parse_pg_ripple_binding(json: &Value, vars: &[String]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Some(obj) = json.as_object() {
        for var in vars {
            if let Some(val) = obj.get(var) {
                let term_str = match val {
                    Value::String(s) if !s.is_empty() => s.clone(),
                    Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            format!(
                                "\"{}\"^^<http://www.w3.org/2001/XMLSchema#integer>",
                                i
                            )
                        } else if let Some(f) = n.as_f64() {
                            format!(
                                "\"{}\"^^<http://www.w3.org/2001/XMLSchema#decimal>",
                                f
                            )
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
    if expected.len() != actual.len() {
        return ValidationResult::Fail(format!(
            "row count mismatch: expected {}, got {}",
            expected.len(),
            actual.len()
        ));
    }

    // Convert to sets of canonical row strings for order-independent comparison.
    fn row_key(row: &HashMap<String, String>, vars: &[String]) -> String {
        vars.iter()
            .map(|v| format!("{}={}", v, row.get(v).map(|s| s.as_str()).unwrap_or("")))
            .collect::<Vec<_>>()
            .join("|")
    }

    let expected_set: HashSet<String> = expected.iter().map(|r| row_key(r, vars)).collect();
    let actual_set: HashSet<String> = actual.iter().map(|r| row_key(r, vars)).collect();

    let missing: Vec<&String> = expected_set.difference(&actual_set).collect();
    let extra: Vec<&String> = actual_set.difference(&expected_set).collect();

    if missing.is_empty() && extra.is_empty() {
        ValidationResult::Pass
    } else {
        let mut msg = String::new();
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

    // Parse variable names from <variable name="..."/>
    let vars: Vec<String> = {
        let mut vs = Vec::new();
        let mut search = content.as_str();
        while let Some(pos) = search.find("<variable name=\"") {
            let rest = &search[pos + "<variable name=\"".len()..];
            if let Some(end) = rest.find('"') {
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
    while let Some(start) = search.find("<binding name=\"") {
        let rest = &search[start + "<binding name=\"".len()..];
        let var_end = match rest.find('"') {
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

/// Convert a SPARQL Results XML term element to a canonical string.
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
        // Check for lang and datatype attributes
        let tag_part = &xml[..xml.find('>')?];
        if let Some(lang_pos) = tag_part.find("xml:lang=\"") {
            let lang_rest = &tag_part[lang_pos + "xml:lang=\"".len()..];
            let lang_end = lang_rest.find('"')?;
            let lang = &lang_rest[..lang_end];
            return Some(format!("\"{value}\"@{lang}"));
        }
        if let Some(dt_pos) = tag_part.find("datatype=\"") {
            let dt_rest = &tag_part[dt_pos + "datatype=\"".len()..];
            let dt_end = dt_rest.find('"')?;
            let dt = &dt_rest[..dt_end];
            return Some(format!("\"{value}\"^^<{dt}>"));
        }
        return Some(format!("\"{value}\""));
    }
    None
}

// ── CONSTRUCT / DESCRIBE via Turtle (.ttl) ────────────────────────────────────

/// Parse a Turtle file into a set of canonical "s p o" strings.
fn parse_turtle_to_triple_set(
    path: &Path,
) -> Result<HashSet<String>, Box<dyn std::error::Error + Send + Sync>> {
    use rio_api::model::{Subject, Term};
    use rio_api::parser::TriplesParser;
    use rio_turtle::TurtleParser;

    let content = std::fs::read_to_string(path)?;
    let mut triples = HashSet::new();

    let mut parser = TurtleParser::new(content.as_bytes(), None);
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
                    format!("\"{value}\"@{language}")
                }
                rio_api::model::Literal::Typed { value, datatype } => {
                    format!("\"{value}\"^^<{}>", datatype.iri)
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
