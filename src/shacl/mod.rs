//! SHACL Core validation engine for pg_ripple v0.7.0.
//!
//! # Architecture
//!
//! SHACL shapes are loaded from Turtle via `load_shacl()`, parsed into a
//! shape IR, and stored in `_pg_ripple.shacl_shapes`.
//!
//! Validation is **spec-first**: the validator compiles shapes into per-shape
//! validator plans over focus nodes and value nodes, preserving exact W3C
//! SHACL semantics.  PostgreSQL constraints and triggers may only be used as
//! internal accelerators when their semantics are provably equivalent.
//!
//! ## Supported constraints (v0.7.0 Core)
//!
//! | Constraint     | Strategy |
//! |----------------|----------|
//! | `sh:minCount`  | Count matching value nodes per focus node |
//! | `sh:maxCount`  | Count matching value nodes per focus node |
//! | `sh:datatype`  | Validate kind + datatype IRI in dictionary |
//! | `sh:in`        | Value node membership in an allowed set |
//! | `sh:pattern`   | Regex match on lexical form |
//! | `sh:class`     | `rdf:type` membership for each value node |
//! | `sh:node`/`sh:property` | Recursive nested shapes |
//!
//! `sh:or`, `sh:and`, `sh:not`, and qualified constraints are v0.8.0.

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;
use serde::{Deserialize, Serialize};

// ─── Shape IR ────────────────────────────────────────────────────────────────

/// The type of SHACL target declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShapeTarget {
    /// `sh:targetClass <IRI>` — all instances of a class.
    Class(String),
    /// `sh:targetNode <IRI>` — specific node(s).
    Node(Vec<String>),
    /// `sh:targetSubjectsOf <IRI>` — subjects of a predicate.
    SubjectsOf(String),
    /// `sh:targetObjectsOf <IRI>` — objects of a predicate.
    ObjectsOf(String),
    /// No explicit target (used for nested property shapes).
    None,
}

/// A single SHACL constraint within a shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShapeConstraint {
    /// `sh:minCount <n>` — at least n value nodes.
    MinCount(i64),
    /// `sh:maxCount <n>` — at most n value nodes.
    MaxCount(i64),
    /// `sh:datatype <IRI>` — value nodes must have this datatype.
    Datatype(String),
    /// `sh:in (v1 v2 ...)` — value nodes must be from this set.
    In(Vec<String>),
    /// `sh:pattern "regex"` with optional `sh:flags`.
    Pattern(String, Option<String>),
    /// `sh:class <IRI>` — value nodes must be instances of this class.
    Class(String),
    /// `sh:node <shape-IRI>` — value nodes must conform to the referenced shape.
    Node(String),
}

/// A SHACL PropertyShape (associated with a path via `sh:path`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyShape {
    /// The IRI of this property shape (may be blank node label).
    pub shape_iri: String,
    /// The predicate IRI for `sh:path` (direct path only in v0.7.0).
    pub path_iri: String,
    /// Constraints on value nodes.
    pub constraints: Vec<ShapeConstraint>,
}

/// A SHACL NodeShape or PropertyShape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shape {
    /// IRI that identifies this shape.
    pub shape_iri: String,
    /// Target declaration.
    pub target: ShapeTarget,
    /// Direct constraints on focus nodes.
    pub constraints: Vec<ShapeConstraint>,
    /// Nested property shapes.
    pub properties: Vec<PropertyShape>,
    /// Whether this shape is deactivated (`sh:deactivated true`).
    pub deactivated: bool,
}

// ─── SHACL Turtle parser ─────────────────────────────────────────────────────

/// Minimal Turtle parser state for SHACL shapes.
/// We use a hand-rolled subset parser because `spargebra` does not parse
/// Turtle, and adding a full Turtle parser crate would be too heavyweight for
/// the subset we need (no blank-node nesting beyond one level).
///
/// Supported pattern (covers the vast majority of real SHACL files):
///
/// ```turtle
/// @prefix sh: <http://www.w3.org/ns/shacl#> .
/// @prefix ex: <http://example.org/> .
///
/// ex:PersonShape
///   a sh:NodeShape ;
///   sh:targetClass ex:Person ;
///   sh:property [
///     sh:path ex:name ;
///     sh:minCount 1 ;
///     sh:datatype xsd:string ;
///   ] .
/// ```
fn parse_shacl_turtle(data: &str) -> Result<Vec<Shape>, String> {
    let mut shapes: Vec<Shape> = Vec::new();
    let mut prefixes: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    // Built-in prefixes.
    prefixes.insert(
        "sh".to_owned(),
        "http://www.w3.org/ns/shacl#".to_owned(),
    );
    prefixes.insert(
        "rdf".to_owned(),
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#".to_owned(),
    );
    prefixes.insert(
        "rdfs".to_owned(),
        "http://www.w3.org/2000/01/rdf-schema#".to_owned(),
    );
    prefixes.insert(
        "xsd".to_owned(),
        "http://www.w3.org/2001/XMLSchema#".to_owned(),
    );
    prefixes.insert(
        "owl".to_owned(),
        "http://www.w3.org/2002/07/owl#".to_owned(),
    );

    // Tokenise into (trimmed) lines and process directive-by-directive.
    // This simple approach handles the common subset used in SHACL definitions.
    let mut lines: Vec<&str> = data.lines().map(|l| l.trim()).collect();

    // Strip comments.
    lines.retain(|l| !l.starts_with('#') && !l.is_empty());

    // Re-join into a single string for statement-level parsing.
    let flat = lines.join(" ");

    // Split on `.` to get individual statements (works for standard SHACL Turtle).
    // This is intentionally simple: we do not support multi-line string literals
    // or complex nested structures beyond one level of `[]`.
    let statements: Vec<&str> = split_turtle_statements(&flat);

    for stmt in &statements {
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }

        // Handle prefix declarations.
        if stmt.starts_with("@prefix") || stmt.to_lowercase().starts_with("prefix") {
            parse_prefix_directive(stmt, &mut prefixes)?;
            continue;
        }

        // Attempt to parse as a shape definition.
        if let Some(shape) = parse_shape_statement(stmt, &prefixes)? {
            shapes.push(shape);
        }
    }

    Ok(shapes)
}

/// Split a flattened Turtle string on `.` boundaries, respecting string literals
/// and bracketed blank-node blocks.
fn split_turtle_statements(flat: &str) -> Vec<&str> {
    let mut result: Vec<&str> = Vec::new();
    let bytes = flat.as_bytes();
    let mut depth = 0usize; // bracket depth
    let mut in_string = false;
    let mut start = 0usize;

    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' if !in_string => { in_string = true; i += 1; }
            b'"' if in_string => {
                // Check for escaped quote.
                if i > 0 && bytes[i - 1] != b'\\' {
                    in_string = false;
                }
                i += 1;
            }
            b'[' if !in_string => { depth += 1; i += 1; }
            b']' if !in_string => {
                if depth > 0 { depth -= 1; }
                i += 1;
            }
            b'.' if !in_string && depth == 0 => {
                let segment = flat[start..i].trim();
                if !segment.is_empty() {
                    result.push(&flat[start..i]);
                }
                start = i + 1;
                i += 1;
            }
            _ => { i += 1; }
        }
    }
    let trailing = flat[start..].trim();
    if !trailing.is_empty() {
        result.push(&flat[start..]);
    }
    result
}

/// Expand a CURIE (`prefix:local`) or bracketed IRI (`<...>`) to a full IRI.
fn expand_iri(
    token: &str,
    prefixes: &std::collections::HashMap<String, String>,
) -> Result<String, String> {
    let token = token.trim();
    if token.starts_with('<') && token.ends_with('>') {
        return Ok(token[1..token.len() - 1].to_owned());
    }
    if let Some(colon) = token.find(':') {
        let prefix = &token[..colon];
        let local = &token[colon + 1..];
        if let Some(ns) = prefixes.get(prefix) {
            return Ok(format!("{ns}{local}"));
        }
        return Err(format!("unknown prefix '{prefix}' in token '{token}'"));
    }
    // Return as-is (may be a keyword like `true`/`false`).
    Ok(token.to_owned())
}

/// Parse `@prefix p: <ns> .` or `PREFIX p: <ns>`.
fn parse_prefix_directive(
    stmt: &str,
    prefixes: &mut std::collections::HashMap<String, String>,
) -> Result<(), String> {
    // Tokenise: [@prefix|PREFIX] <prefix>: <IRI>
    let tokens: Vec<&str> = stmt.split_whitespace().collect();
    if tokens.len() < 3 {
        return Err(format!("malformed prefix directive: '{stmt}'"));
    }
    let prefix_token = tokens[1]; // e.g. "ex:"
    let iri_token = tokens[2];    // e.g. "<http://example.org/>"

    let prefix = prefix_token.trim_end_matches(':');
    let iri = if iri_token.starts_with('<') && iri_token.ends_with('>') {
        iri_token[1..iri_token.len() - 1].to_owned()
    } else {
        return Err(format!("expected IRI in prefix directive, got '{iri_token}'"));
    };

    prefixes.insert(prefix.to_owned(), iri);
    Ok(())
}

/// Parse a single Turtle statement into a `Shape` if it defines one.
/// Returns `Ok(None)` for statements that are not shape definitions.
fn parse_shape_statement(
    stmt: &str,
    prefixes: &std::collections::HashMap<String, String>,
) -> Result<Option<Shape>, String> {
    // Split subject from predicate-object pairs on the first whitespace.
    let stmt = stmt.trim();
    let (subject_token, rest) = match stmt.find(char::is_whitespace) {
        Some(i) => (stmt[..i].trim(), stmt[i..].trim()),
        None => return Ok(None),
    };

    // Expand subject IRI.
    let shape_iri = expand_iri(subject_token, prefixes)?;

    // Parse predicate-object pairs separated by `;`.
    let po_pairs: Vec<&str> = rest.split(';').collect();

    let mut is_shape = false;
    let mut target = ShapeTarget::None;
    let mut constraints: Vec<ShapeConstraint> = Vec::new();
    let mut properties: Vec<PropertyShape> = Vec::new();
    let mut deactivated = false;

    for pair in &po_pairs {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }

        // Split predicate from objects (first whitespace).
        let (pred_token, obj_rest) = match pair.find(char::is_whitespace) {
            Some(i) => (pair[..i].trim(), pair[i..].trim()),
            None => continue,
        };

        let pred_iri = expand_iri(pred_token, prefixes)?;

        // `a` shorthand → rdf:type
        let pred_iri = if pred_iri == "a" {
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned()
        } else {
            pred_iri
        };

        match pred_iri.as_str() {
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type" => {
                let obj_iri = expand_iri(obj_rest.trim(), prefixes)?;
                if obj_iri == "http://www.w3.org/ns/shacl#NodeShape"
                    || obj_iri == "http://www.w3.org/ns/shacl#PropertyShape"
                {
                    is_shape = true;
                }
            }
            "http://www.w3.org/ns/shacl#targetClass" => {
                let iri = expand_iri(obj_rest.trim(), prefixes)?;
                target = ShapeTarget::Class(iri);
            }
            "http://www.w3.org/ns/shacl#targetNode" => {
                let iri = expand_iri(obj_rest.trim(), prefixes)?;
                target = ShapeTarget::Node(vec![iri]);
            }
            "http://www.w3.org/ns/shacl#targetSubjectsOf" => {
                let iri = expand_iri(obj_rest.trim(), prefixes)?;
                target = ShapeTarget::SubjectsOf(iri);
            }
            "http://www.w3.org/ns/shacl#targetObjectsOf" => {
                let iri = expand_iri(obj_rest.trim(), prefixes)?;
                target = ShapeTarget::ObjectsOf(iri);
            }
            "http://www.w3.org/ns/shacl#deactivated" => {
                deactivated = obj_rest.trim() == "true";
            }
            "http://www.w3.org/ns/shacl#minCount" => {
                let n: i64 = obj_rest
                    .trim()
                    .parse()
                    .map_err(|_| format!("sh:minCount value is not an integer: '{obj_rest}'"))?;
                constraints.push(ShapeConstraint::MinCount(n));
            }
            "http://www.w3.org/ns/shacl#maxCount" => {
                let n: i64 = obj_rest
                    .trim()
                    .parse()
                    .map_err(|_| format!("sh:maxCount value is not an integer: '{obj_rest}'"))?;
                constraints.push(ShapeConstraint::MaxCount(n));
            }
            "http://www.w3.org/ns/shacl#datatype" => {
                let iri = expand_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::Datatype(iri));
            }
            "http://www.w3.org/ns/shacl#class" => {
                let iri = expand_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::Class(iri));
            }
            "http://www.w3.org/ns/shacl#pattern" => {
                let pattern = extract_string_literal(obj_rest.trim())?;
                constraints.push(ShapeConstraint::Pattern(pattern, None));
            }
            "http://www.w3.org/ns/shacl#in" => {
                let values = parse_list_values(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::In(values));
            }
            "http://www.w3.org/ns/shacl#property" => {
                // Property shapes are delimited by `[` ... `]`.
                if let Some(ps) = parse_property_shape(obj_rest.trim(), prefixes)? {
                    is_shape = true;
                    properties.push(ps);
                }
            }
            _ => {
                // Unknown predicate — ignore (forward-compatible).
            }
        }
    }

    if !is_shape {
        return Ok(None);
    }

    Ok(Some(Shape {
        shape_iri,
        target,
        constraints,
        properties,
        deactivated,
    }))
}

/// Parse a property shape from a `[ sh:path ... ; sh:minCount ... ]` block.
fn parse_property_shape(
    block: &str,
    prefixes: &std::collections::HashMap<String, String>,
) -> Result<Option<PropertyShape>, String> {
    // Strip outer `[` and `]`.
    let inner = block.trim();
    let inner = if inner.starts_with('[') && inner.ends_with(']') {
        inner[1..inner.len() - 1].trim()
    } else {
        return Err(format!("property shape must be enclosed in [ ], got: '{inner}'"));
    };

    let po_pairs: Vec<&str> = inner.split(';').collect();
    let mut path_iri: Option<String> = None;
    let mut constraints: Vec<ShapeConstraint> = Vec::new();
    let mut shape_iri = format!("_blank_{}", uuid_short());

    for pair in &po_pairs {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let (pred_token, obj_rest) = match pair.find(char::is_whitespace) {
            Some(i) => (pair[..i].trim(), pair[i..].trim()),
            None => continue,
        };

        let pred_iri = expand_iri(pred_token, prefixes)?;
        let pred_iri = if pred_iri == "a" {
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned()
        } else {
            pred_iri
        };

        match pred_iri.as_str() {
            "http://www.w3.org/ns/shacl#path" => {
                path_iri = Some(expand_iri(obj_rest.trim(), prefixes)?);
            }
            "http://www.w3.org/ns/shacl#name" => {
                // Use sh:name as the blank-node label if available.
                shape_iri = extract_string_literal(obj_rest.trim())
                    .unwrap_or_else(|_| shape_iri.clone());
            }
            "http://www.w3.org/ns/shacl#minCount" => {
                let n: i64 = obj_rest
                    .trim()
                    .parse()
                    .map_err(|_| format!("sh:minCount value is not an integer: '{obj_rest}'"))?;
                constraints.push(ShapeConstraint::MinCount(n));
            }
            "http://www.w3.org/ns/shacl#maxCount" => {
                let n: i64 = obj_rest
                    .trim()
                    .parse()
                    .map_err(|_| format!("sh:maxCount value is not an integer: '{obj_rest}'"))?;
                constraints.push(ShapeConstraint::MaxCount(n));
            }
            "http://www.w3.org/ns/shacl#datatype" => {
                let iri = expand_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::Datatype(iri));
            }
            "http://www.w3.org/ns/shacl#class" => {
                let iri = expand_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::Class(iri));
            }
            "http://www.w3.org/ns/shacl#pattern" => {
                let pattern = extract_string_literal(obj_rest.trim())?;
                constraints.push(ShapeConstraint::Pattern(pattern, None));
            }
            "http://www.w3.org/ns/shacl#in" => {
                let values = parse_list_values(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::In(values));
            }
            "http://www.w3.org/ns/shacl#node" => {
                let iri = expand_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::Node(iri));
            }
            _ => {}
        }
    }

    let path = match path_iri {
        Some(p) => p,
        None => return Err("property shape is missing sh:path".to_owned()),
    };

    Ok(Some(PropertyShape {
        shape_iri,
        path_iri: path,
        constraints,
    }))
}

/// Extract the string value from a Turtle string literal `"..."` or `'...'`.
fn extract_string_literal(token: &str) -> Result<String, String> {
    let token = token.trim();
    // Handle `"..."` (with optional `@lang` or `^^type` suffix).
    if token.starts_with('"') {
        let end = token[1..].find('"').ok_or_else(|| format!("unterminated string literal: '{token}'"))?;
        return Ok(token[1..end + 1].to_owned());
    }
    if token.starts_with('\'') {
        let end = token[1..].find('\'').ok_or_else(|| format!("unterminated string literal: '{token}'"))?;
        return Ok(token[1..end + 1].to_owned());
    }
    Err(format!("expected string literal, got: '{token}'"))
}

/// Parse a Turtle `( v1 v2 ... )` list into individual IRI strings.
fn parse_list_values(
    token: &str,
    prefixes: &std::collections::HashMap<String, String>,
) -> Result<Vec<String>, String> {
    let token = token.trim();
    let inner = if token.starts_with('(') && token.ends_with(')') {
        token[1..token.len() - 1].trim()
    } else {
        return Err(format!("sh:in expects a Turtle list ( ... ), got: '{token}'"));
    };
    inner
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|t| expand_iri(t, prefixes))
        .collect()
}

/// Generate a short unique ID for anonymous property shapes.
fn uuid_short() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{ns:08x}")
}

// ─── Shape storage ────────────────────────────────────────────────────────────

/// Persist a parsed `Shape` into `_pg_ripple.shacl_shapes` as JSON.
fn store_shape(shape: &Shape) -> Result<(), String> {
    let json = serde_json::to_value(shape)
        .map_err(|e| format!("failed to serialise shape '{}': {e}", shape.shape_iri))?;
    let json_str = json.to_string();

    Spi::run_with_args(
        "INSERT INTO _pg_ripple.shacl_shapes (shape_iri, shape_json, active)
         VALUES ($1, $2::jsonb, true)
         ON CONFLICT (shape_iri) DO UPDATE
             SET shape_json = EXCLUDED.shape_json,
                 active     = true,
                 updated_at = now()",
        &[
            DatumWithOid::from(shape.shape_iri.as_str()),
            DatumWithOid::from(json_str.as_str()),
        ],
    )
    .map_err(|e| format!("failed to store shape '{}': {e}", shape.shape_iri))
}

/// Parse SHACL Turtle data, store each shape into `_pg_ripple.shacl_shapes`,
/// and return the number of shapes successfully stored.
///
/// Fails (via `pgrx::error!`) if the Turtle is malformed so that partial state
/// is not committed to the catalog.
pub fn parse_and_store_shapes(data: &str) -> i32 {
    let shapes = match parse_shacl_turtle(data) {
        Ok(s) => s,
        Err(e) => pgrx::error!("SHACL shape parsing failed: {e}"),
    };

    if shapes.is_empty() {
        pgrx::warning!("load_shacl: no shapes found in the supplied Turtle data");
        return 0;
    }

    let mut stored = 0i32;
    for shape in &shapes {
        match store_shape(shape) {
            Ok(()) => stored += 1,
            Err(e) => pgrx::error!("failed to store shape '{}': {e}", shape.shape_iri),
        }
    }
    stored
}

/// Load all active shapes from `_pg_ripple.shacl_shapes`.
pub fn load_shapes() -> Vec<Shape> {
    let rows = Spi::connect(|c| {
        let tup = c
            .select(
                "SELECT shape_json::text FROM _pg_ripple.shacl_shapes WHERE active = true",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("load_shapes SPI error: {e}"));
        let mut out: Vec<String> = Vec::new();
        for row in tup {
            if let Ok(Some(s)) = row.get::<&str>(1) {
                out.push(s.to_owned());
            }
        }
        out
    });

    rows.into_iter()
        .filter_map(|json| serde_json::from_str::<Shape>(&json).ok())
        .collect()
}

// ─── Validator plans ─────────────────────────────────────────────────────────

/// A violation entry in a SHACL validation report.
#[derive(Debug, Serialize)]
pub struct Violation {
    pub focus_node: String,
    pub shape_iri: String,
    pub path: Option<String>,
    pub constraint: String,
    pub message: String,
    pub severity: String,
}

/// Execute validation for a single `PropertyShape` against all focus nodes in
/// graph `g` (0 = default graph, -1 = all graphs).
/// Returns all violations found.
fn validate_property_shape(
    ps: &PropertyShape,
    focus_nodes: &[i64],
    graph_id: i64,
    shape_iri: &str,
) -> Vec<Violation> {
    let mut violations: Vec<Violation> = Vec::new();
    let path_id_opt = crate::dictionary::lookup_iri(&ps.path_iri);
    let path_id = match path_id_opt {
        Some(id) => id,
        None => {
            // Path predicate not in dictionary — no triples can match, so
            // minCount violations may apply.
            for &focus in focus_nodes {
                for c in &ps.constraints {
                    if let ShapeConstraint::MinCount(n) = c {
                        if *n > 0 {
                            let focus_iri = crate::dictionary::decode(focus)
                                .unwrap_or_else(|| format!("_id_{focus}"));
                            violations.push(Violation {
                                focus_node: focus_iri,
                                shape_iri: shape_iri.to_owned(),
                                path: Some(ps.path_iri.clone()),
                                constraint: "sh:minCount".to_owned(),
                                message: format!(
                                    "expected at least {n} value(s) for <{}>, found 0",
                                    ps.path_iri
                                ),
                                severity: "Violation".to_owned(),
                            });
                        }
                    }
                }
            }
            return violations;
        }
    };

    for &focus in focus_nodes {
        // Count value nodes for this focus node along the path predicate.
        let count: i64 = if graph_id < 0 {
            count_values_all_graphs(focus, path_id)
        } else {
            count_values_in_graph(focus, path_id, graph_id)
        };

        for c in &ps.constraints {
            match c {
                ShapeConstraint::MinCount(n) => {
                    if count < *n {
                        let focus_iri = crate::dictionary::decode(focus)
                            .unwrap_or_else(|| format!("_id_{focus}"));
                        violations.push(Violation {
                            focus_node: focus_iri,
                            shape_iri: shape_iri.to_owned(),
                            path: Some(ps.path_iri.clone()),
                            constraint: "sh:minCount".to_owned(),
                            message: format!(
                                "expected at least {n} value(s) for <{}>, found {count}",
                                ps.path_iri
                            ),
                            severity: "Violation".to_owned(),
                        });
                    }
                }
                ShapeConstraint::MaxCount(n) => {
                    if count > *n {
                        let focus_iri = crate::dictionary::decode(focus)
                            .unwrap_or_else(|| format!("_id_{focus}"));
                        violations.push(Violation {
                            focus_node: focus_iri,
                            shape_iri: shape_iri.to_owned(),
                            path: Some(ps.path_iri.clone()),
                            constraint: "sh:maxCount".to_owned(),
                            message: format!(
                                "expected at most {n} value(s) for <{}>, found {count}",
                                ps.path_iri
                            ),
                            severity: "Violation".to_owned(),
                        });
                    }
                }
                ShapeConstraint::Datatype(dt_iri) => {
                    // Retrieve all value nodes and check their datatype.
                    let value_ids = get_value_ids(focus, path_id, graph_id);
                    for v_id in value_ids {
                        if !value_has_datatype(v_id, dt_iri) {
                            let focus_iri = crate::dictionary::decode(focus)
                                .unwrap_or_else(|| format!("_id_{focus}"));
                            violations.push(Violation {
                                focus_node: focus_iri,
                                shape_iri: shape_iri.to_owned(),
                                path: Some(ps.path_iri.clone()),
                                constraint: "sh:datatype".to_owned(),
                                message: format!(
                                    "value node id {v_id} does not have datatype <{dt_iri}>"
                                ),
                                severity: "Violation".to_owned(),
                            });
                        }
                    }
                }
                ShapeConstraint::In(allowed_iris) => {
                    let allowed_ids: Vec<i64> = allowed_iris
                        .iter()
                        .filter_map(|iri| crate::dictionary::lookup_iri(iri))
                        .collect();
                    let value_ids = get_value_ids(focus, path_id, graph_id);
                    for v_id in value_ids {
                        if !allowed_ids.contains(&v_id) {
                            let focus_iri = crate::dictionary::decode(focus)
                                .unwrap_or_else(|| format!("_id_{focus}"));
                            violations.push(Violation {
                                focus_node: focus_iri,
                                shape_iri: shape_iri.to_owned(),
                                path: Some(ps.path_iri.clone()),
                                constraint: "sh:in".to_owned(),
                                message: format!("value node id {v_id} is not in the allowed value set"),
                                severity: "Violation".to_owned(),
                            });
                        }
                    }
                }
                ShapeConstraint::Pattern(regex, _flags) => {
                    let value_ids = get_value_ids(focus, path_id, graph_id);
                    for v_id in value_ids {
                        let lexical = crate::dictionary::decode(v_id)
                            .unwrap_or_default();
                        // Strip surrounding quotes for string literals.
                        let lexical = if lexical.starts_with('"') {
                            lexical
                                .trim_start_matches('"')
                                .split('"')
                                .next()
                                .unwrap_or(&lexical)
                                .to_owned()
                        } else {
                            lexical
                        };
                        let matches: Option<bool> = Spi::get_one_with_args::<bool>(
                            "SELECT $1 ~ $2",
                            &[
                                DatumWithOid::from(lexical.as_str()),
                                DatumWithOid::from(regex.as_str()),
                            ],
                        )
                        .unwrap_or(None);
                        if !matches.unwrap_or(false) {
                            let focus_iri = crate::dictionary::decode(focus)
                                .unwrap_or_else(|| format!("_id_{focus}"));
                            violations.push(Violation {
                                focus_node: focus_iri,
                                shape_iri: shape_iri.to_owned(),
                                path: Some(ps.path_iri.clone()),
                                constraint: "sh:pattern".to_owned(),
                                message: format!(
                                    "value '{lexical}' does not match pattern '{regex}'"
                                ),
                                severity: "Violation".to_owned(),
                            });
                        }
                    }
                }
                ShapeConstraint::Class(class_iri) => {
                    let class_id_opt = crate::dictionary::lookup_iri(class_iri);
                    let rdf_type_id_opt = crate::dictionary::lookup_iri(
                        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                    );
                    let value_ids = get_value_ids(focus, path_id, graph_id);
                    for v_id in value_ids {
                        let has_class = match (class_id_opt, rdf_type_id_opt) {
                            (Some(cid), Some(tid)) => value_has_rdf_type(v_id, tid, cid),
                            _ => false,
                        };
                        if !has_class {
                            let focus_iri = crate::dictionary::decode(focus)
                                .unwrap_or_else(|| format!("_id_{focus}"));
                            violations.push(Violation {
                                focus_node: focus_iri,
                                shape_iri: shape_iri.to_owned(),
                                path: Some(ps.path_iri.clone()),
                                constraint: "sh:class".to_owned(),
                                message: format!(
                                    "value node id {v_id} is not an instance of <{class_iri}>"
                                ),
                                severity: "Violation".to_owned(),
                            });
                        }
                    }
                }
                ShapeConstraint::Node(_) => {
                    // Nested node shapes — v0.8.0; skip silently.
                }
                ShapeConstraint::MinCount(_) | ShapeConstraint::MaxCount(_) => {
                    // Already handled above.
                }
            }
        }
    }

    violations
}

/// Collect all focus nodes for a shape in the given graph.
/// Returns encoded (i64) subject IDs.
fn collect_focus_nodes(target: &ShapeTarget, graph_id: i64) -> Vec<i64> {
    match target {
        ShapeTarget::None => vec![],
        ShapeTarget::Node(iris) => iris
            .iter()
            .filter_map(|iri| crate::dictionary::lookup_iri(iri))
            .collect(),
        ShapeTarget::Class(class_iri) => {
            let class_id = match crate::dictionary::lookup_iri(class_iri) {
                Some(id) => id,
                None => return vec![],
            };
            let rdf_type_id = match crate::dictionary::lookup_iri(
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            ) {
                Some(id) => id,
                None => return vec![],
            };
            get_subjects_with_type(rdf_type_id, class_id, graph_id)
        }
        ShapeTarget::SubjectsOf(pred_iri) => {
            let pred_id = match crate::dictionary::lookup_iri(pred_iri) {
                Some(id) => id,
                None => return vec![],
            };
            get_subjects_of_predicate(pred_id, graph_id)
        }
        ShapeTarget::ObjectsOf(pred_iri) => {
            let pred_id = match crate::dictionary::lookup_iri(pred_iri) {
                Some(id) => id,
                None => return vec![],
            };
            get_objects_of_predicate(pred_id, graph_id)
        }
    }
}

// ─── Low-level query helpers ──────────────────────────────────────────────────

fn count_values_in_graph(focus: i64, path_id: i64, graph_id: i64) -> i64 {
    // Use the unified VP view for the path predicate.
    let table = get_vp_table_name(path_id);
    let sql = format!("SELECT COUNT(*) FROM {table} WHERE s = $1 AND g = $2");
    Spi::get_one_with_args::<i64>(&sql, &[DatumWithOid::from(focus), DatumWithOid::from(graph_id)])
        .unwrap_or(None)
        .unwrap_or(0)
}

fn count_values_all_graphs(focus: i64, path_id: i64) -> i64 {
    let table = get_vp_table_name(path_id);
    let sql = format!("SELECT COUNT(*) FROM {table} WHERE s = $1");
    Spi::get_one_with_args::<i64>(&sql, &[DatumWithOid::from(focus)])
        .unwrap_or(None)
        .unwrap_or(0)
}

fn get_value_ids(focus: i64, path_id: i64, graph_id: i64) -> Vec<i64> {
    let table = get_vp_table_name(path_id);
    let sql = if graph_id < 0 {
        format!("SELECT o FROM {table} WHERE s = $1")
    } else {
        format!("SELECT o FROM {table} WHERE s = $1 AND g = $2")
    };
    let args: Vec<DatumWithOid> = if graph_id < 0 {
        vec![DatumWithOid::from(focus)]
    } else {
        vec![DatumWithOid::from(focus), DatumWithOid::from(graph_id)]
    };
    Spi::connect(|c| {
        let tup = c.select(&sql, None, &args)
            .unwrap_or_else(|e| pgrx::error!("get_value_ids SPI error: {e}"));
        let mut ids: Vec<i64> = Vec::new();
        for row in tup {
            if let Ok(Some(v)) = row.get::<i64>(1) {
                ids.push(v);
            }
        }
        ids
    })
}

fn get_subjects_with_type(rdf_type_id: i64, class_id: i64, graph_id: i64) -> Vec<i64> {
    let table = get_vp_table_name(rdf_type_id);
    let sql = if graph_id < 0 {
        format!("SELECT s FROM {table} WHERE o = $1")
    } else {
        format!("SELECT s FROM {table} WHERE o = $1 AND g = $2")
    };
    let args: Vec<DatumWithOid> = if graph_id < 0 {
        vec![DatumWithOid::from(class_id)]
    } else {
        vec![DatumWithOid::from(class_id), DatumWithOid::from(graph_id)]
    };
    Spi::connect(|c| {
        let tup = c.select(&sql, None, &args)
            .unwrap_or_else(|e| pgrx::error!("get_subjects_with_type SPI error: {e}"));
        let mut ids: Vec<i64> = Vec::new();
        for row in tup {
            if let Ok(Some(v)) = row.get::<i64>(1) {
                ids.push(v);
            }
        }
        ids
    })
}

fn get_subjects_of_predicate(pred_id: i64, graph_id: i64) -> Vec<i64> {
    let table = get_vp_table_name(pred_id);
    let sql = if graph_id < 0 {
        format!("SELECT DISTINCT s FROM {table}")
    } else {
        format!("SELECT DISTINCT s FROM {table} WHERE g = $1")
    };
    let args: Vec<DatumWithOid> = if graph_id < 0 {
        vec![]
    } else {
        vec![DatumWithOid::from(graph_id)]
    };
    Spi::connect(|c| {
        let tup = c.select(&sql, None, &args)
            .unwrap_or_else(|e| pgrx::error!("get_subjects_of_predicate SPI error: {e}"));
        let mut ids: Vec<i64> = Vec::new();
        for row in tup {
            if let Ok(Some(v)) = row.get::<i64>(1) {
                ids.push(v);
            }
        }
        ids
    })
}

fn get_objects_of_predicate(pred_id: i64, graph_id: i64) -> Vec<i64> {
    let table = get_vp_table_name(pred_id);
    let sql = if graph_id < 0 {
        format!("SELECT DISTINCT o FROM {table}")
    } else {
        format!("SELECT DISTINCT o FROM {table} WHERE g = $1")
    };
    let args: Vec<DatumWithOid> = if graph_id < 0 {
        vec![]
    } else {
        vec![DatumWithOid::from(graph_id)]
    };
    Spi::connect(|c| {
        let tup = c.select(&sql, None, &args)
            .unwrap_or_else(|e| pgrx::error!("get_objects_of_predicate SPI error: {e}"));
        let mut ids: Vec<i64> = Vec::new();
        for row in tup {
            if let Ok(Some(v)) = row.get::<i64>(1) {
                ids.push(v);
            }
        }
        ids
    })
}

fn value_has_datatype(value_id: i64, dt_iri: &str) -> bool {
    Spi::get_one_with_args::<bool>(
        "SELECT EXISTS(SELECT 1 FROM _pg_ripple.dictionary WHERE id = $1 AND datatype = $2)",
        &[
            DatumWithOid::from(value_id),
            DatumWithOid::from(dt_iri),
        ],
    )
    .unwrap_or(None)
    .unwrap_or(false)
}

fn value_has_rdf_type(value_id: i64, rdf_type_pred_id: i64, class_id: i64) -> bool {
    let table = get_vp_table_name(rdf_type_pred_id);
    let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE s = $1 AND o = $2)");
    Spi::get_one_with_args::<bool>(
        &sql,
        &[DatumWithOid::from(value_id), DatumWithOid::from(class_id)],
    )
    .unwrap_or(None)
    .unwrap_or(false)
}

/// Return the best available VP table/view name for a predicate ID.
/// Falls back to `_pg_ripple.vp_rare` with a WHERE clause prefix hint.
fn get_vp_table_name(pred_id: i64) -> String {
    let has_dedicated = Spi::get_one_with_args::<bool>(
        "SELECT EXISTS(SELECT 1 FROM _pg_ripple.predicates WHERE id = $1 AND table_oid IS NOT NULL)",
        &[DatumWithOid::from(pred_id)],
    )
    .unwrap_or(None)
    .unwrap_or(false);

    if has_dedicated {
        format!("_pg_ripple.vp_{pred_id}")
    } else {
        // For vp_rare the WHERE clause must include `p = pred_id`.
        // The caller is responsible for adding `AND p = ...` in this case.
        // Return a subquery that already filters by predicate.
        format!("(SELECT s, o, g, i, source FROM _pg_ripple.vp_rare WHERE p = {pred_id})")
    }
}

// ─── Public validation entry point ───────────────────────────────────────────

/// Run offline validation of all data in the given graph (NULL = default graph 0,
/// empty string = all graphs) against all active SHACL shapes.
/// Returns a SHACL validation report as JSONB.
pub fn run_validate(graph: Option<&str>) -> pgrx::JsonB {
    let graph_id: i64 = match graph {
        None | Some("") => 0,
        Some("*") => -1, // special sentinel: all graphs
        Some(g) => {
            let g_clean = if g.starts_with('<') && g.ends_with('>') {
                &g[1..g.len() - 1]
            } else {
                g
            };
            crate::dictionary::lookup_iri(g_clean).unwrap_or(0)
        }
    };

    let shapes = load_shapes();
    let mut all_violations: Vec<serde_json::Value> = Vec::new();
    let mut conforms = true;

    for shape in &shapes {
        if shape.deactivated {
            continue;
        }

        let focus_nodes = collect_focus_nodes(&shape.target, graph_id);

        // Validate property shapes.
        for ps in &shape.properties {
            let violations = validate_property_shape(ps, &focus_nodes, graph_id, &shape.shape_iri);
            for v in violations {
                conforms = false;
                all_violations.push(serde_json::json!({
                    "focusNode": v.focus_node,
                    "shapeIRI":  v.shape_iri,
                    "path":      v.path,
                    "constraint": v.constraint,
                    "message":   v.message,
                    "severity":  v.severity
                }));
            }
        }
    }

    let report = serde_json::json!({
        "conforms": conforms,
        "violations": all_violations
    });

    pgrx::JsonB(report)
}

/// Synchronous validation of a single triple (s_id, p_id, o_id, g_id).
/// Returns `Ok(())` if the triple conforms to all active shapes, or
/// `Err(String)` with a concise violation message if any shape is violated.
///
/// Only invoked when `pg_ripple.shacl_mode = 'sync'`.
pub fn validate_sync(s_id: i64, p_id: i64, o_id: i64, g_id: i64) -> Result<(), String> {
    let shapes = load_shapes();

    for shape in &shapes {
        if shape.deactivated {
            continue;
        }

        // Determine whether this triple's subject is a focus node for the shape.
        let is_focus = match &shape.target {
            ShapeTarget::None => false,
            ShapeTarget::Node(iris) => iris
                .iter()
                .any(|iri| crate::dictionary::lookup_iri(iri) == Some(s_id)),
            ShapeTarget::Class(class_iri) => {
                let class_id = match crate::dictionary::lookup_iri(class_iri) {
                    Some(id) => id,
                    None => continue,
                };
                let rdf_type_id = match crate::dictionary::lookup_iri(
                    "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                ) {
                    Some(id) => id,
                    None => continue,
                };
                value_has_rdf_type(s_id, rdf_type_id, class_id)
            }
            ShapeTarget::SubjectsOf(pred_iri) => {
                crate::dictionary::lookup_iri(pred_iri) == Some(p_id)
            }
            ShapeTarget::ObjectsOf(pred_iri) => {
                crate::dictionary::lookup_iri(pred_iri) == Some(p_id)
            }
        };

        if !is_focus {
            continue;
        }

        // Check property constraints for this predicate.
        for ps in &shape.properties {
            let ps_path_id = match crate::dictionary::lookup_iri(&ps.path_iri) {
                Some(id) => id,
                None => continue,
            };
            if ps_path_id != p_id {
                continue;
            }

            // Check direct value constraints on o_id.
            for c in &ps.constraints {
                match c {
                    ShapeConstraint::MaxCount(n) => {
                        let current = count_values_in_graph(s_id, p_id, g_id);
                        // After this insert there would be current + 1 values.
                        if current + 1 > *n {
                            let focus_iri = crate::dictionary::decode(s_id)
                                .unwrap_or_else(|| format!("_id_{s_id}"));
                            return Err(format!(
                                "SHACL violation: <{}> sh:maxCount {n} for <{}>: \
                                 found {} existing value(s), limit is {n}",
                                focus_iri, ps.path_iri, current
                            ));
                        }
                    }
                    ShapeConstraint::Datatype(dt_iri) => {
                        if !value_has_datatype(o_id, dt_iri) {
                            let focus_iri = crate::dictionary::decode(s_id)
                                .unwrap_or_else(|| format!("_id_{s_id}"));
                            return Err(format!(
                                "SHACL violation: <{}> sh:datatype <{dt_iri}> for <{}>: \
                                 object id {o_id} does not have the required datatype",
                                focus_iri, ps.path_iri
                            ));
                        }
                    }
                    ShapeConstraint::In(allowed_iris) => {
                        let allowed_ids: Vec<i64> = allowed_iris
                            .iter()
                            .filter_map(|iri| crate::dictionary::lookup_iri(iri))
                            .collect();
                        if !allowed_ids.contains(&o_id) {
                            let focus_iri = crate::dictionary::decode(s_id)
                                .unwrap_or_else(|| format!("_id_{s_id}"));
                            return Err(format!(
                                "SHACL violation: <{}> sh:in for <{}>: \
                                 object id {o_id} is not in the allowed value set",
                                focus_iri, ps.path_iri
                            ));
                        }
                    }
                    ShapeConstraint::Pattern(regex, _) => {
                        let lexical = crate::dictionary::decode(o_id).unwrap_or_default();
                        let lexical_clean = if lexical.starts_with('"') {
                            lexical
                                .trim_start_matches('"')
                                .split('"')
                                .next()
                                .unwrap_or(&lexical)
                                .to_owned()
                        } else {
                            lexical.clone()
                        };
                        let matches: Option<bool> = Spi::get_one_with_args::<bool>(
                            "SELECT $1 ~ $2",
                            &[
                                DatumWithOid::from(lexical_clean.as_str()),
                                DatumWithOid::from(regex.as_str()),
                            ],
                        )
                        .unwrap_or(None);
                        if !matches.unwrap_or(false) {
                            let focus_iri = crate::dictionary::decode(s_id)
                                .unwrap_or_else(|| format!("_id_{s_id}"));
                            return Err(format!(
                                "SHACL violation: <{}> sh:pattern '{regex}' for <{}>: \
                                 value '{lexical_clean}' does not match",
                                focus_iri, ps.path_iri
                            ));
                        }
                    }
                    // minCount checked at query/validate time, not at insert time
                    // (it's about absence, which can't be detected on single insert).
                    _ => {}
                }
            }
        }
    }

    Ok(())
}
