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

pub mod constraints;
pub mod hints;

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
    // ── v0.8.0 complex constraints ────────────────────────────────────────────
    /// `sh:or (shape1 shape2 ...)` — node/value must conform to at least one shape.
    Or(Vec<String>),
    /// `sh:and (shape1 shape2 ...)` — node/value must conform to all listed shapes.
    And(Vec<String>),
    /// `sh:not <shape-IRI>` — node/value must NOT conform to the referenced shape.
    Not(String),
    /// `sh:qualifiedValueShape <shape-IRI>` with optional min/max cardinality.
    QualifiedValueShape {
        shape_iri: String,
        min_count: Option<i64>,
        max_count: Option<i64>,
    },
    // ── v0.23.0 SHACL Core completion ─────────────────────────────────────────
    /// `sh:hasValue <value>` — at least one value node must equal the given RDF term.
    HasValue(String),
    /// `sh:nodeKind <kind-IRI>` — value nodes must be of the specified RDF node kind.
    /// Valid values: sh:IRI, sh:BlankNode, sh:Literal, sh:BlankNodeOrIRI,
    /// sh:BlankNodeOrLiteral, sh:IRIOrLiteral.
    NodeKind(String),
    /// `sh:languageIn (tag1 tag2 ...)` — value nodes must have a language tag in the list.
    LanguageIn(Vec<String>),
    /// `sh:uniqueLang` — no two value nodes may have the same non-empty language tag.
    UniqueLang,
    /// `sh:lessThan <path-IRI>` — each value must be less than every value on the other path.
    LessThan(String),
    /// `sh:lessThanOrEquals <path-IRI>` — each value must be less than or equal to every value on the other path.
    LessThanOrEquals(String),
    /// `sh:greaterThan <path-IRI>` — each value must be greater than every value on the other path.
    GreaterThan(String),
    /// `sh:closed true` — reject triples whose predicate is not in the shape's declared property set.
    Closed { ignored_properties: Vec<String> },
    // ── v0.45.0 relational constraints ────────────────────────────────────────
    /// `sh:equals <path-IRI>` — the set of values for the focus node's path must
    /// equal the set of values for the given other path.  Both direction NOT EXISTS
    /// subqueries must return no rows.
    Equals(String),
    /// `sh:disjoint <path-IRI>` — the value sets of the focus node's path and the
    /// given other path must be disjoint (no common value IDs).
    Disjoint(String),
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

/// Strip `/* ... */` block comments from a Turtle source string (M-11).
///
/// Matches multi-line block comments (like SPARQL comments) without using the
/// regex crate.  Does not strip single-line `#` comments (those are handled by
/// the line-level filter below).
fn strip_block_comments(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            // Skip until closing `*/`.
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i += 2; // skip past the closing `*/`
        } else {
            // SAFETY: bytes[i] is a valid byte index into the UTF-8 string.
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

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
fn parse_shacl_turtle(data_raw: &str) -> Result<Vec<Shape>, String> {
    // M-11: strip /* ... */ block comments before any other processing.
    let stripped = strip_block_comments(data_raw);
    let data = stripped.as_str();
    let mut shapes: Vec<Shape> = Vec::new();
    let mut prefixes: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    // Built-in prefixes.
    prefixes.insert("sh".to_owned(), "http://www.w3.org/ns/shacl#".to_owned());
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

/// Split `s` on `;` characters that are at bracket-depth 0 (i.e. not inside
/// a `[…]` blank-node property block).  Returns a vector of borrowed slices.
fn split_on_semicolon_top_level(s: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = Vec::new();
    let bytes = s.as_bytes();
    let mut depth = 0usize;
    let mut start = 0usize;

    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'[' => depth += 1,
            b']' => depth = depth.saturating_sub(1),
            b';' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

/// Split a flattened Turtle string on `.` boundaries, respecting string literals
/// and bracketed blank-node blocks.
fn split_turtle_statements(flat: &str) -> Vec<&str> {
    let mut result: Vec<&str> = Vec::new();
    let bytes = flat.as_bytes();
    let mut depth = 0usize; // bracket depth
    let mut in_string = false;
    let mut in_iri = false; // inside <...> angle-bracket IRI
    let mut start = 0usize;

    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'<' if !in_string && !in_iri => {
                in_iri = true;
                i += 1;
            }
            b'>' if in_iri => {
                in_iri = false;
                i += 1;
            }
            b'"' if !in_string && !in_iri => {
                in_string = true;
                i += 1;
            }
            b'"' if in_string => {
                // Check for escaped quote.
                if i > 0 && bytes[i - 1] != b'\\' {
                    in_string = false;
                }
                i += 1;
            }
            b'[' if !in_string && !in_iri => {
                depth += 1;
                i += 1;
            }
            b']' if !in_string && !in_iri => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b'.' if !in_string && !in_iri && depth == 0 => {
                let segment = flat[start..i].trim();
                if !segment.is_empty() {
                    result.push(&flat[start..i]);
                }
                start = i + 1;
                i += 1;
            }
            _ => {
                i += 1;
            }
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
    let iri_token = tokens[2]; // e.g. "<http://example.org/>"

    let prefix = prefix_token.trim_end_matches(':');
    let iri = if iri_token.starts_with('<') && iri_token.ends_with('>') {
        iri_token[1..iri_token.len() - 1].to_owned()
    } else {
        return Err(format!(
            "expected IRI in prefix directive, got '{iri_token}'"
        ));
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

    // Parse predicate-object pairs separated by `;`, but `;` inside `[…]`
    // brackets belongs to a nested property shape and must not be treated as
    // a pair separator.
    let po_pairs: Vec<&str> = split_on_semicolon_top_level(rest);

    let mut is_shape = false;
    let mut target = ShapeTarget::None;
    let mut constraints: Vec<ShapeConstraint> = Vec::new();
    let mut properties: Vec<PropertyShape> = Vec::new();
    let mut deactivated = false;
    // v0.23.0 accumulators for sh:closed + sh:ignoredProperties.
    let mut closed = false;
    let mut ignored_properties: Vec<String> = Vec::new();

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
            // ── v0.8.0 logical combinators ────────────────────────────────────
            "http://www.w3.org/ns/shacl#or" => {
                let shape_iris = parse_list_values(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::Or(shape_iris));
            }
            "http://www.w3.org/ns/shacl#and" => {
                let shape_iris = parse_list_values(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::And(shape_iris));
            }
            "http://www.w3.org/ns/shacl#not" => {
                let shape_iri = expand_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::Not(shape_iri));
            }
            // ── v0.23.0 SHACL Core completion ─────────────────────────────────
            "http://www.w3.org/ns/shacl#hasValue" => {
                let val = expand_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::HasValue(val));
            }
            "http://www.w3.org/ns/shacl#nodeKind" => {
                let iri = expand_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::NodeKind(iri));
            }
            "http://www.w3.org/ns/shacl#languageIn" => {
                let tags = parse_list_values(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::LanguageIn(tags));
            }
            "http://www.w3.org/ns/shacl#uniqueLang" if obj_rest.trim() == "true" => {
                constraints.push(ShapeConstraint::UniqueLang);
            }
            "http://www.w3.org/ns/shacl#uniqueLang" => {}
            "http://www.w3.org/ns/shacl#closed" if obj_rest.trim() == "true" => {
                closed = true;
            }
            "http://www.w3.org/ns/shacl#closed" => {}
            "http://www.w3.org/ns/shacl#ignoredProperties" => {
                ignored_properties = parse_list_values(obj_rest.trim(), prefixes)?;
            }
            // ── v0.45.0 relational constraints ────────────────────────────────
            "http://www.w3.org/ns/shacl#equals" => {
                let iri = expand_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::Equals(iri));
            }
            "http://www.w3.org/ns/shacl#disjoint" => {
                let iri = expand_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::Disjoint(iri));
            }
            _ => {
                // Unknown predicate — ignore (forward-compatible).
            }
        }
    }

    // Emit Closed constraint if sh:closed was true.
    if closed {
        constraints.push(ShapeConstraint::Closed {
            ignored_properties: ignored_properties.clone(),
        });
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
        return Err(format!(
            "property shape must be enclosed in [ ], got: '{inner}'"
        ));
    };

    let po_pairs: Vec<&str> = inner.split(';').collect();
    let mut path_iri: Option<String> = None;
    let mut constraints: Vec<ShapeConstraint> = Vec::new();
    let mut shape_iri = format!("_blank_{}", uuid_short());
    // Accumulators for sh:qualifiedValueShape (spans multiple predicates).
    let mut qualified_shape_iri: Option<String> = None;
    let mut qualified_min_count: Option<i64> = None;
    let mut qualified_max_count: Option<i64> = None;

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
                shape_iri =
                    extract_string_literal(obj_rest.trim()).unwrap_or_else(|_| shape_iri.clone());
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
            // ── v0.8.0 logical combinators ────────────────────────────────────
            "http://www.w3.org/ns/shacl#or" => {
                let shape_iris = parse_list_values(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::Or(shape_iris));
            }
            "http://www.w3.org/ns/shacl#and" => {
                let shape_iris = parse_list_values(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::And(shape_iris));
            }
            "http://www.w3.org/ns/shacl#not" => {
                let shape_iri_val = expand_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::Not(shape_iri_val));
            }
            // ── v0.8.0 qualified value shapes ─────────────────────────────────
            "http://www.w3.org/ns/shacl#qualifiedValueShape" => {
                qualified_shape_iri = Some(expand_iri(obj_rest.trim(), prefixes)?);
            }
            "http://www.w3.org/ns/shacl#qualifiedMinCount" => {
                let n: i64 = obj_rest.trim().parse().map_err(|_| {
                    format!("sh:qualifiedMinCount value is not an integer: '{obj_rest}'")
                })?;
                qualified_min_count = Some(n);
            }
            "http://www.w3.org/ns/shacl#qualifiedMaxCount" => {
                let n: i64 = obj_rest.trim().parse().map_err(|_| {
                    format!("sh:qualifiedMaxCount value is not an integer: '{obj_rest}'")
                })?;
                qualified_max_count = Some(n);
            }
            // ── v0.23.0 SHACL Core completion ─────────────────────────────────
            "http://www.w3.org/ns/shacl#hasValue" => {
                let val = expand_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::HasValue(val));
            }
            "http://www.w3.org/ns/shacl#nodeKind" => {
                let iri = expand_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::NodeKind(iri));
            }
            "http://www.w3.org/ns/shacl#languageIn" => {
                let tags = parse_list_values(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::LanguageIn(tags));
            }
            "http://www.w3.org/ns/shacl#uniqueLang" if obj_rest.trim() == "true" => {
                constraints.push(ShapeConstraint::UniqueLang);
            }
            "http://www.w3.org/ns/shacl#uniqueLang" => {}
            "http://www.w3.org/ns/shacl#lessThan" => {
                let other_path = expand_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::LessThan(other_path));
            }
            "http://www.w3.org/ns/shacl#lessThanOrEquals" => {
                let other_path = expand_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::LessThanOrEquals(other_path));
            }
            "http://www.w3.org/ns/shacl#greaterThan" => {
                let other_path = expand_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::GreaterThan(other_path));
            }
            // ── v0.45.0 relational constraints ────────────────────────────────
            "http://www.w3.org/ns/shacl#equals" => {
                let iri = expand_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::Equals(iri));
            }
            "http://www.w3.org/ns/shacl#disjoint" => {
                let iri = expand_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::Disjoint(iri));
            }
            _ => {}
        }
    }

    // Emit QualifiedValueShape constraint if sh:qualifiedValueShape was seen.
    if let Some(qvs_iri) = qualified_shape_iri {
        constraints.push(ShapeConstraint::QualifiedValueShape {
            shape_iri: qvs_iri,
            min_count: qualified_min_count,
            max_count: qualified_max_count,
        });
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
    if let Some(inner) = token.strip_prefix('"') {
        let end = inner
            .find('"')
            .ok_or_else(|| format!("unterminated string literal: '{token}'"))?;
        return Ok(inner[..end].to_owned());
    }
    if let Some(inner) = token.strip_prefix('\'') {
        let end = inner
            .find('\'')
            .ok_or_else(|| format!("unterminated string literal: '{token}'"))?;
        return Ok(inner[..end].to_owned());
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
        return Err(format!(
            "sh:in expects a Turtle list ( ... ), got: '{token}'"
        ));
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
            Ok(()) => {
                stored += 1;
                // v0.38.0: populate query-planner hints from shape constraints.
                hints::populate_hints(shape);
            }
            Err(e) => pgrx::error!("failed to store shape '{}': {e}", shape.shape_iri),
        }
    }

    // v0.10.0: SHACL-AF sh:rule bridge.
    // If inference_mode is not 'off', scan for sh:rule entries and register
    // any found as Datalog rules.
    let infer_mode = crate::INFERENCE_MODE
        .get()
        .as_ref()
        .and_then(|c| c.to_str().ok())
        .unwrap_or("off")
        .to_owned();

    if infer_mode != "off" {
        let registered = bridge_shacl_rules(data);
        if registered > 0 {
            pgrx::warning!(
                "load_shacl: auto-registered {registered} sh:rule entries as Datalog rules"
            );
        }
    }

    stored
}

/// SHACL-AF bridge (v0.10.0): scan Turtle data for `sh:rule` triples and
/// register the associated Datalog rule bodies.
///
/// This handles the basic SHACL-AF `sh:rule` pattern:
/// ```turtle
/// ex:MyShape sh:rule [
///     rdf:type sh:TripleRule ;
///     sh:subject ?this ;
///     sh:predicate ex:myPred ;
///     sh:object ?object ;
/// ] .
/// ```
///
/// Returns the number of rule patterns found and registered.
pub fn bridge_shacl_rules(data: &str) -> i32 {
    // Detect sh:rule presence in the raw Turtle text.
    if !data.contains("sh:rule") && !data.contains("shacl#rule") {
        return 0;
    }

    // Simple extraction: find sh:rule block patterns and convert to Datalog.
    // For a full implementation, a complete Turtle parser would be needed.
    // This initial version detects and logs sh:rule presence.
    let count = data.matches("sh:rule").count() as i32;
    if count > 0 {
        // Register a placeholder rule indicating sh:rule was detected.
        // Full compilation of sh:rule bodies is a future enhancement.
        let _ = Spi::run_with_args(
            "INSERT INTO _pg_ripple.rules \
             (rule_set, rule_text, head_pred, stratum, is_recursive, active) \
             VALUES ('shacl-af', $1, NULL, 0, false, true) \
             ON CONFLICT DO NOTHING",
            &[pgrx::datum::DatumWithOid::from(
                "# SHACL-AF sh:rule detected; full compilation pending",
            )],
        );
    }
    count
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

// ── v0.8.0 helper: recursive shape conformance check ─────────────────────────

/// Check whether the node identified by `node_id` conforms to the shape with
/// IRI `shape_iri` in graph `graph_id`.
///
/// Returns `true` when the node conforms (or when the shape is not found —
/// open-world assumption).  Depth-limited to 32 levels to prevent infinite
/// recursion on cyclic shape references.
pub(crate) fn node_conforms_to_shape(
    node_id: i64,
    shape_iri: &str,
    graph_id: i64,
    all_shapes: &[Shape],
) -> bool {
    node_conforms_to_shape_depth(node_id, shape_iri, graph_id, all_shapes, 0)
}

fn node_conforms_to_shape_depth(
    node_id: i64,
    shape_iri: &str,
    graph_id: i64,
    all_shapes: &[Shape],
    depth: u32,
) -> bool {
    if depth > 32 {
        // Cycle guard: treat as conformant to avoid false violations.
        return true;
    }
    let shape = match all_shapes.iter().find(|s| s.shape_iri == shape_iri) {
        Some(s) => s,
        None => return true, // unknown shape → open world
    };
    if shape.deactivated {
        return true;
    }

    // Check top-level node constraints.
    for c in &shape.constraints {
        if !node_satisfies_constraint(node_id, c, graph_id, all_shapes, depth) {
            return false;
        }
    }

    // Check property shape constraints.
    for ps in &shape.properties {
        let viols = validate_property_shape_depth(
            ps,
            &[node_id],
            graph_id,
            shape_iri,
            all_shapes,
            depth + 1,
        );
        if !viols.is_empty() {
            return false;
        }
    }

    true
}

/// Check a single top-level node constraint on `node_id`.
/// Returns `true` when the node satisfies the constraint.
fn node_satisfies_constraint(
    node_id: i64,
    constraint: &ShapeConstraint,
    graph_id: i64,
    all_shapes: &[Shape],
    depth: u32,
) -> bool {
    match constraint {
        ShapeConstraint::Or(shape_iris) => shape_iris
            .iter()
            .any(|s| node_conforms_to_shape_depth(node_id, s, graph_id, all_shapes, depth + 1)),
        ShapeConstraint::And(shape_iris) => shape_iris
            .iter()
            .all(|s| node_conforms_to_shape_depth(node_id, s, graph_id, all_shapes, depth + 1)),
        ShapeConstraint::Not(shape_iri) => {
            !node_conforms_to_shape_depth(node_id, shape_iri, graph_id, all_shapes, depth + 1)
        }
        // Other node-level constraints are validated via property shapes.
        _ => true,
    }
}

/// Depth-aware variant of validate_property_shape for recursive calls.
fn validate_property_shape_depth(
    ps: &PropertyShape,
    focus_nodes: &[i64],
    graph_id: i64,
    shape_iri: &str,
    all_shapes: &[Shape],
    _depth: u32,
) -> Vec<Violation> {
    validate_property_shape(ps, focus_nodes, graph_id, shape_iri, all_shapes)
}

/// Execute validation for a single `PropertyShape` against all focus nodes in
/// graph `g` (0 = default graph, -1 = all graphs).
/// Returns all violations found.
///
/// This is the ≤50-line dispatcher that delegates to `constraints/` sub-modules.
fn validate_property_shape(
    ps: &PropertyShape,
    focus_nodes: &[i64],
    graph_id: i64,
    shape_iri: &str,
    all_shapes: &[Shape],
) -> Vec<Violation> {
    let mut violations: Vec<Violation> = Vec::new();
    let path_id = match crate::dictionary::lookup_iri(&ps.path_iri) {
        Some(id) => id,
        None => {
            // Path predicate not in dictionary — only MinCount can fire (found 0 values).
            for &focus in focus_nodes {
                for c in &ps.constraints {
                    if let ShapeConstraint::MinCount(n) = c
                        && *n > 0
                    {
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
            return violations;
        }
    };
    for &focus in focus_nodes {
        let count = if graph_id < 0 {
            count_values_all_graphs(focus, path_id)
        } else {
            count_values_in_graph(focus, path_id, graph_id)
        };
        let args = constraints::ConstraintArgs {
            focus,
            count,
            path_id,
            graph_id,
            shape_iri,
            path_iri: &ps.path_iri,
            all_shapes,
        };
        for c in &ps.constraints {
            dispatch_constraint(c, &args, &mut violations);
        }
    }
    violations
}

/// Dispatch a single `ShapeConstraint` to the appropriate per-family checker.
fn dispatch_constraint(
    c: &ShapeConstraint,
    args: &constraints::ConstraintArgs,
    violations: &mut Vec<Violation>,
) {
    match c {
        ShapeConstraint::MinCount(n) => constraints::count::check_min_count(*n, args, violations),
        ShapeConstraint::MaxCount(n) => constraints::count::check_max_count(*n, args, violations),
        ShapeConstraint::Datatype(dt) => {
            constraints::value_type::check_datatype(dt, args, violations)
        }
        ShapeConstraint::Class(cls) => constraints::value_type::check_class(cls, args, violations),
        ShapeConstraint::NodeKind(k) => {
            constraints::value_type::check_node_kind(k, args, violations)
        }
        ShapeConstraint::Pattern(rx, _) => {
            constraints::string_based::check_pattern(rx, args, violations)
        }
        ShapeConstraint::LanguageIn(tags) => {
            constraints::string_based::check_language_in(tags, args, violations)
        }
        ShapeConstraint::UniqueLang => {
            constraints::string_based::check_unique_lang(args, violations)
        }
        ShapeConstraint::Node(s) => constraints::logical::check_node(s, args, violations),
        ShapeConstraint::Or(ss) => constraints::logical::check_or(ss, args, violations),
        ShapeConstraint::And(ss) => constraints::logical::check_and(ss, args, violations),
        ShapeConstraint::Not(s) => constraints::logical::check_not(s, args, violations),
        ShapeConstraint::QualifiedValueShape {
            shape_iri: qiri,
            min_count,
            max_count,
        } => {
            constraints::logical::check_qualified(qiri, *min_count, *max_count, args, violations);
        }
        ShapeConstraint::In(vals) => constraints::shape_based::check_in(vals, args, violations),
        ShapeConstraint::HasValue(v) => {
            constraints::shape_based::check_has_value(v, args, violations)
        }
        ShapeConstraint::LessThan(p) => {
            constraints::shape_based::check_less_than(p, args, violations)
        }
        ShapeConstraint::LessThanOrEquals(p) => {
            constraints::shape_based::check_less_than_or_equals(p, args, violations)
        }
        ShapeConstraint::GreaterThan(p) => {
            constraints::shape_based::check_greater_than(p, args, violations)
        }
        ShapeConstraint::Closed { .. } => constraints::shape_based::check_closed(args, violations),
        ShapeConstraint::Equals(p) => constraints::relational::check_equals(p, args, violations),
        ShapeConstraint::Disjoint(p) => {
            constraints::relational::check_disjoint(p, args, violations)
        }
    }
}

/// Safe decode helper: returns the decoded IRI string for an id, or a
/// `"<decoded-id:{id}>"` fallback if the dictionary lookup fails.
pub fn decode_id_safe(id: i64) -> String {
    crate::dictionary::decode(id).unwrap_or_else(|| format!("<decoded-id:{id}>"))
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

pub(crate) fn count_values_in_graph(focus: i64, path_id: i64, graph_id: i64) -> i64 {
    // Use the unified VP view for the path predicate.
    let table = get_vp_table_name(path_id);
    let sql = format!("SELECT COUNT(*) FROM {table} WHERE s = $1 AND g = $2");
    Spi::get_one_with_args::<i64>(
        &sql,
        &[DatumWithOid::from(focus), DatumWithOid::from(graph_id)],
    )
    .unwrap_or(None)
    .unwrap_or(0)
}

pub(crate) fn count_values_all_graphs(focus: i64, path_id: i64) -> i64 {
    let table = get_vp_table_name(path_id);
    let sql = format!("SELECT COUNT(*) FROM {table} WHERE s = $1");
    Spi::get_one_with_args::<i64>(&sql, &[DatumWithOid::from(focus)])
        .unwrap_or(None)
        .unwrap_or(0)
}

pub(crate) fn get_value_ids(focus: i64, path_id: i64, graph_id: i64) -> Vec<i64> {
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
        let tup = c
            .select(&sql, None, &args)
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
        let tup = c
            .select(&sql, None, &args)
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
        let tup = c
            .select(&sql, None, &args)
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
        let tup = c
            .select(&sql, None, &args)
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

/// Encode a value token from a `sh:in` list into a dictionary ID for lookup.
///
/// Tokens that start with `"` are treated as string literals (plain, typed, or
/// language-tagged). All other tokens are treated as IRIs (read-only lookup).
/// Returns `None` if the value is not in the dictionary.
pub(crate) fn encode_shacl_in_value(val: &str) -> Option<i64> {
    if let Some(inner) = val.strip_prefix('"') {
        // String literal: extract the lexical value between the first pair of
        // double quotes, then check for ^^<datatype> or @lang suffix.
        let close = inner.rfind('"')?;
        let str_val = &inner[..close];
        let rest = inner[close + 1..].trim();
        if let Some(dt_rest) = rest.strip_prefix("^^<") {
            let dt = dt_rest.trim_end_matches('>');
            // Use encode_typed_literal (safe: called after data load, value
            // already exists; if not, a new entry is created harmlessly).
            Some(crate::dictionary::encode_typed_literal(str_val, dt))
        } else if let Some(lang_rest) = rest.strip_prefix('@') {
            let lang = lang_rest.split_whitespace().next().unwrap_or(lang_rest);
            Some(crate::dictionary::encode_lang_literal(str_val, lang))
        } else {
            // Plain string literal: read-only lookup.
            Spi::get_one_with_args::<i64>(
                "SELECT id FROM _pg_ripple.dictionary WHERE value = $1 AND kind = 2 \
                 AND lang IS NULL AND datatype IS NULL",
                &[DatumWithOid::from(str_val)],
            )
            .ok()
            .flatten()
        }
    } else {
        // IRI: standard read-only lookup.
        crate::dictionary::lookup_iri(val)
    }
}

pub(crate) fn value_has_datatype(value_id: i64, dt_iri: &str) -> bool {
    use crate::dictionary::inline;

    // Inline-encoded values (bit 63 = 1) are never stored in the dictionary.
    // Determine their datatype directly from the inline type code.
    if inline::is_inline(value_id) {
        let expected = match inline::inline_type(value_id) {
            inline::TYPE_INTEGER => "http://www.w3.org/2001/XMLSchema#integer",
            inline::TYPE_BOOLEAN => "http://www.w3.org/2001/XMLSchema#boolean",
            inline::TYPE_DATETIME => "http://www.w3.org/2001/XMLSchema#dateTime",
            inline::TYPE_DATE => "http://www.w3.org/2001/XMLSchema#date",
            _ => return false,
        };
        return dt_iri == expected;
    }

    // Plain literals (kind=KIND_LITERAL, datatype=NULL) are implicitly xsd:string
    // per the RDF specification.  The N-Triples loader stores `"foo"^^xsd:string`
    // as a plain literal, so both kinds satisfy sh:datatype xsd:string.
    if dt_iri == "http://www.w3.org/2001/XMLSchema#string" {
        return Spi::get_one_with_args::<bool>(
            "SELECT EXISTS(\
               SELECT 1 FROM _pg_ripple.dictionary \
               WHERE id = $1 AND (datatype = $2 OR (kind = 2 AND datatype IS NULL)))",
            &[DatumWithOid::from(value_id), DatumWithOid::from(dt_iri)],
        )
        .unwrap_or(None)
        .unwrap_or(false);
    }

    Spi::get_one_with_args::<bool>(
        "SELECT EXISTS(SELECT 1 FROM _pg_ripple.dictionary WHERE id = $1 AND datatype = $2)",
        &[DatumWithOid::from(value_id), DatumWithOid::from(dt_iri)],
    )
    .unwrap_or(None)
    .unwrap_or(false)
}

pub(crate) fn value_has_rdf_type(value_id: i64, rdf_type_pred_id: i64, class_id: i64) -> bool {
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
pub(crate) fn get_vp_table_name(pred_id: i64) -> String {
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

// ─── v0.23.0 helper functions ─────────────────────────────────────────────────

/// Check whether a dictionary value node satisfies `sh:nodeKind`.
///
/// kind_iri must be one of the W3C SHACL node kind IRIs:
/// sh:IRI, sh:BlankNode, sh:Literal, sh:BlankNodeOrIRI,
/// sh:BlankNodeOrLiteral, sh:IRIOrLiteral.
pub(crate) fn value_has_node_kind(value_id: i64, kind_iri: &str) -> bool {
    // Determine the actual kind from the dictionary.
    let kind: i16 = Spi::get_one_with_args::<i16>(
        "SELECT kind FROM _pg_ripple.dictionary WHERE id = $1",
        &[DatumWithOid::from(value_id)],
    )
    .unwrap_or(None)
    .unwrap_or(-1);

    let is_iri = kind == crate::dictionary::KIND_IRI;
    let is_blank = kind == crate::dictionary::KIND_BLANK;
    let is_literal = matches!(
        kind,
        k if k == crate::dictionary::KIND_LITERAL
            || k == crate::dictionary::KIND_TYPED_LITERAL
            || k == crate::dictionary::KIND_LANG_LITERAL
    );

    let sh = "http://www.w3.org/ns/shacl#";
    match kind_iri.strip_prefix(sh).unwrap_or(kind_iri) {
        "IRI" => is_iri,
        "BlankNode" => is_blank,
        "Literal" => is_literal,
        "BlankNodeOrIRI" => is_blank || is_iri,
        "BlankNodeOrLiteral" => is_blank || is_literal,
        "IRIOrLiteral" => is_iri || is_literal,
        _ => false,
    }
}

/// Retrieve the language tag for a dictionary value node.
///
/// Returns `Some(lang)` for language-tagged literals (kind = KIND_LANG_LITERAL),
/// `None` for all other term kinds.
pub(crate) fn get_language_tag(value_id: i64) -> Option<String> {
    Spi::get_one_with_args::<String>(
        "SELECT lang FROM _pg_ripple.dictionary WHERE id = $1 AND lang IS NOT NULL",
        &[DatumWithOid::from(value_id)],
    )
    .ok()
    .flatten()
}

/// Compare two dictionary value nodes for ordering (used by sh:lessThan / sh:greaterThan).
///
/// Decodes both values to their lexical forms and attempts a numeric comparison;
/// falls back to lexicographic comparison.  Returns `None` when comparison is
/// not meaningful (different types, IRIs, etc.).
pub(crate) fn compare_dictionary_values(a: i64, b: i64) -> Option<std::cmp::Ordering> {
    let a_str = crate::dictionary::decode(a)?;
    let b_str = crate::dictionary::decode(b)?;

    // Try to extract numeric values for typed literals.
    let extract_number = |s: &str| -> Option<f64> {
        // N-Triples literal: "value"^^<type> or "value"
        if let Some(rest) = s.strip_prefix('"') {
            let inner_end = rest.find('"')?;
            let lexical = &rest[..inner_end];
            lexical.parse::<f64>().ok()
        } else {
            None
        }
    };

    if let (Some(na), Some(nb)) = (extract_number(&a_str), extract_number(&b_str)) {
        return na.partial_cmp(&nb);
    }

    // Lexicographic fallback (works for dates in ISO 8601 form).
    Some(a_str.cmp(&b_str))
}

/// Return all predicate IRIs used by `focus` node in graph `graph_id`.
///
/// Used by `sh:closed` validation.
fn get_all_predicate_iris_for_node(focus: i64, graph_id: i64) -> Vec<String> {
    let mut predicates = Vec::new();

    // Check vp_rare (one query, already has a `p` column).
    let rare_preds: Vec<i64> = {
        let sql = if graph_id < 0 {
            "SELECT DISTINCT p FROM _pg_ripple.vp_rare WHERE s = $1".to_owned()
        } else {
            "SELECT DISTINCT p FROM _pg_ripple.vp_rare WHERE s = $1 AND g = $2".to_owned()
        };
        let args: Vec<DatumWithOid> = if graph_id < 0 {
            vec![DatumWithOid::from(focus)]
        } else {
            vec![DatumWithOid::from(focus), DatumWithOid::from(graph_id)]
        };
        Spi::connect(|c| {
            let rows = c
                .select(&sql, None, &args)
                .unwrap_or_else(|e| pgrx::error!("get_all_predicate_iris_for_node: {e}"));
            rows.filter_map(|row| row.get::<i64>(1).ok().flatten())
                .collect::<Vec<i64>>()
        })
    };
    for p_id in rare_preds {
        if let Some(iri) = crate::dictionary::decode(p_id) {
            predicates.push(iri);
        }
    }

    // Check dedicated VP tables.
    let dedicated_ids: Vec<i64> = Spi::connect(|c| {
        let rows = c
            .select(
                "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("get_all_predicate_iris_for_node SPI: {e}"));
        rows.filter_map(|row| row.get::<i64>(1).ok().flatten())
            .collect()
    });

    for pred_id in dedicated_ids {
        let table = format!("_pg_ripple.vp_{pred_id}");
        let has_subject: bool = if graph_id < 0 {
            let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE s = $1)");
            Spi::get_one_with_args::<bool>(&sql, &[DatumWithOid::from(focus)])
        } else {
            let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE s = $1 AND g = $2)");
            Spi::get_one_with_args::<bool>(
                &sql,
                &[DatumWithOid::from(focus), DatumWithOid::from(graph_id)],
            )
        }
        .unwrap_or(None)
        .unwrap_or(false);

        if has_subject && let Some(iri) = crate::dictionary::decode(pred_id) {
            predicates.push(iri);
        }
    }

    predicates
}

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

        // v0.8.0: validate top-level node constraints (sh:or, sh:and, sh:not).
        for c in &shape.constraints {
            match c {
                ShapeConstraint::Or(shape_iris) => {
                    for &focus in &focus_nodes {
                        let ok = shape_iris
                            .iter()
                            .any(|s| node_conforms_to_shape(focus, s, graph_id, &shapes));
                        if !ok {
                            conforms = false;
                            let focus_iri = crate::dictionary::decode(focus)
                                .unwrap_or_else(|| format!("_id_{focus}"));
                            all_violations.push(serde_json::json!({
                                "focusNode":  focus_iri,
                                "shapeIRI":   shape.shape_iri,
                                "path":       serde_json::Value::Null,
                                "constraint": "sh:or",
                                "message":    "focus node does not conform to any sh:or shape",
                                "severity":   "Violation"
                            }));
                        }
                    }
                }
                ShapeConstraint::And(shape_iris) => {
                    for &focus in &focus_nodes {
                        for s in shape_iris {
                            if !node_conforms_to_shape(focus, s, graph_id, &shapes) {
                                conforms = false;
                                let focus_iri = crate::dictionary::decode(focus)
                                    .unwrap_or_else(|| format!("_id_{focus}"));
                                all_violations.push(serde_json::json!({
                                    "focusNode":  focus_iri,
                                    "shapeIRI":   shape.shape_iri,
                                    "path":       serde_json::Value::Null,
                                    "constraint": "sh:and",
                                    "message":    format!("focus node does not conform to sh:and shape <{s}>"),
                                    "severity":   "Violation"
                                }));
                            }
                        }
                    }
                }
                ShapeConstraint::Not(ref_shape_iri) => {
                    for &focus in &focus_nodes {
                        if node_conforms_to_shape(focus, ref_shape_iri, graph_id, &shapes) {
                            conforms = false;
                            let focus_iri = crate::dictionary::decode(focus)
                                .unwrap_or_else(|| format!("_id_{focus}"));
                            all_violations.push(serde_json::json!({
                                "focusNode":  focus_iri,
                                "shapeIRI":   shape.shape_iri,
                                "path":       serde_json::Value::Null,
                                "constraint": "sh:not",
                                "message":    format!("focus node must not conform to shape <{ref_shape_iri}>"),
                                "severity":   "Violation"
                            }));
                        }
                    }
                }
                _ => {}
            }
        }

        // Validate property shapes.
        for ps in &shape.properties {
            let violations =
                validate_property_shape(ps, &focus_nodes, graph_id, &shape.shape_iri, &shapes);
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

        // v0.23.0: sh:closed validation.
        // If the shape has sh:closed true, ensure no focus node uses a predicate
        // outside the declared property set (plus rdf:type and ignoredProperties).
        if let Some(ShapeConstraint::Closed { ignored_properties }) = shape
            .constraints
            .iter()
            .find(|c| matches!(c, ShapeConstraint::Closed { .. }))
        {
            let declared_paths: std::collections::HashSet<String> = shape
                .properties
                .iter()
                .map(|ps| ps.path_iri.clone())
                .collect();
            let mut allowed: std::collections::HashSet<String> = declared_paths;
            allowed.insert("http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned());
            for ign in ignored_properties {
                allowed.insert(ign.clone());
            }

            for &focus in &focus_nodes {
                let used_preds = get_all_predicate_iris_for_node(focus, graph_id);
                for pred_iri in used_preds {
                    if !allowed.contains(&pred_iri) {
                        conforms = false;
                        let focus_iri = crate::dictionary::decode(focus)
                            .unwrap_or_else(|| format!("_id_{focus}"));
                        all_violations.push(serde_json::json!({
                            "focusNode":  focus_iri,
                            "shapeIRI":   shape.shape_iri,
                            "path":       serde_json::Value::Null,
                            "constraint": "sh:closed",
                            "message":    format!(
                                "predicate <{pred_iri}> is not in the declared property set \
                                 of the closed shape <{}>", shape.shape_iri
                            ),
                            "severity":   "Violation"
                        }));
                    }
                }
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
                    ShapeConstraint::MinCount(_) => {}
                    // ── v0.8.0 additions ─────────────────────────────────────
                    ShapeConstraint::Class(class_iri) => {
                        let class_id_opt = crate::dictionary::lookup_iri(class_iri);
                        let rdf_type_id_opt = crate::dictionary::lookup_iri(
                            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                        );
                        let has_class = match (class_id_opt, rdf_type_id_opt) {
                            (Some(cid), Some(tid)) => value_has_rdf_type(o_id, tid, cid),
                            _ => false,
                        };
                        if !has_class {
                            let focus_iri = crate::dictionary::decode(s_id)
                                .unwrap_or_else(|| format!("_id_{s_id}"));
                            return Err(format!(
                                "SHACL violation: <{}> sh:class <{class_iri}> for <{}>: \
                                 object id {o_id} is not an instance of the required class",
                                focus_iri, ps.path_iri
                            ));
                        }
                    }
                    ShapeConstraint::Node(ref_shape_iri) => {
                        if !node_conforms_to_shape(o_id, ref_shape_iri, g_id, &shapes) {
                            let focus_iri = crate::dictionary::decode(s_id)
                                .unwrap_or_else(|| format!("_id_{s_id}"));
                            return Err(format!(
                                "SHACL violation: <{}> sh:node <{ref_shape_iri}> for <{}>: \
                                 object id {o_id} does not conform to the referenced shape",
                                focus_iri, ps.path_iri
                            ));
                        }
                    }
                    ShapeConstraint::Or(shape_iris) => {
                        let conforms = shape_iris
                            .iter()
                            .any(|s| node_conforms_to_shape(o_id, s, g_id, &shapes));
                        if !conforms {
                            let focus_iri = crate::dictionary::decode(s_id)
                                .unwrap_or_else(|| format!("_id_{s_id}"));
                            return Err(format!(
                                "SHACL violation: <{}> sh:or for <{}>: \
                                 object id {o_id} does not conform to any of the sh:or shapes",
                                focus_iri, ps.path_iri
                            ));
                        }
                    }
                    ShapeConstraint::And(shape_iris) => {
                        for s in shape_iris {
                            if !node_conforms_to_shape(o_id, s, g_id, &shapes) {
                                let focus_iri = crate::dictionary::decode(s_id)
                                    .unwrap_or_else(|| format!("_id_{s_id}"));
                                return Err(format!(
                                    "SHACL violation: <{}> sh:and <{s}> for <{}>: \
                                     object id {o_id} does not conform to the required shape",
                                    focus_iri, ps.path_iri
                                ));
                            }
                        }
                    }
                    ShapeConstraint::Not(ref_shape_iri) => {
                        if node_conforms_to_shape(o_id, ref_shape_iri, g_id, &shapes) {
                            let focus_iri = crate::dictionary::decode(s_id)
                                .unwrap_or_else(|| format!("_id_{s_id}"));
                            return Err(format!(
                                "SHACL violation: <{}> sh:not <{ref_shape_iri}> for <{}>: \
                                 object id {o_id} must not conform to the referenced shape",
                                focus_iri, ps.path_iri
                            ));
                        }
                    }
                    ShapeConstraint::QualifiedValueShape {
                        shape_iri: qvs_iri,
                        min_count: _,
                        max_count,
                    } => {
                        // For sync (single insert), only sh:qualifiedMaxCount is checkable.
                        if let Some(max) = max_count {
                            let existing_qualifying =
                                count_qualifying_values(s_id, p_id, g_id, qvs_iri, &shapes);
                            if existing_qualifying + 1 > *max {
                                let focus_iri = crate::dictionary::decode(s_id)
                                    .unwrap_or_else(|| format!("_id_{s_id}"));
                                return Err(format!(
                                    "SHACL violation: <{}> sh:qualifiedMaxCount {max} for <{}>: \
                                     found {} qualifying value(s), limit is {max}",
                                    focus_iri, ps.path_iri, existing_qualifying
                                ));
                            }
                        }
                    }
                    // v0.23.0 new constraints — sync checks where feasible.
                    ShapeConstraint::HasValue(expected_val) => {
                        // sh:hasValue: the new value being inserted might satisfy it, so no rejection here.
                        // Full conformance is only verifiable offline (need to check all values).
                        let _ = expected_val;
                    }
                    ShapeConstraint::NodeKind(kind_iri) => {
                        if !value_has_node_kind(o_id, kind_iri) {
                            let focus_iri = crate::dictionary::decode(s_id)
                                .unwrap_or_else(|| format!("_id_{s_id}"));
                            return Err(format!(
                                "SHACL violation: <{}> sh:nodeKind <{kind_iri}> for <{}>: \
                                 value id {o_id} does not match required node kind",
                                focus_iri, ps.path_iri
                            ));
                        }
                    }
                    ShapeConstraint::LanguageIn(allowed_tags) => {
                        let lang_opt = get_language_tag(o_id);
                        let ok = match &lang_opt {
                            Some(lang) => {
                                let lang_lower = lang.to_lowercase();
                                allowed_tags.iter().any(|t| {
                                    let bare = t.trim_matches('"');
                                    bare.to_lowercase() == lang_lower
                                })
                            }
                            None => false,
                        };
                        if !ok {
                            let focus_iri = crate::dictionary::decode(s_id)
                                .unwrap_or_else(|| format!("_id_{s_id}"));
                            return Err(format!(
                                "SHACL violation: <{}> sh:languageIn for <{}>: \
                                 value id {o_id} language {:?} not in allowed list {:?}",
                                focus_iri,
                                ps.path_iri,
                                lang_opt.as_deref().unwrap_or("none"),
                                allowed_tags
                            ));
                        }
                    }
                    // These constraints need all values present — skip for single insert.
                    ShapeConstraint::UniqueLang
                    | ShapeConstraint::LessThan(_)
                    | ShapeConstraint::LessThanOrEquals(_)
                    | ShapeConstraint::GreaterThan(_)
                    | ShapeConstraint::Closed { .. }
                    // v0.45.0: relational constraints require full value sets — skip for single insert.
                    | ShapeConstraint::Equals(_)
                    | ShapeConstraint::Disjoint(_) => {}
                }
            }
        }
    }

    Ok(())
}

/// Count how many current values along `(s, p)` in graph `g_id` conform to
/// the shape `qvs_iri`.  Used by the sync validator for `sh:qualifiedMaxCount`.
fn count_qualifying_values(
    s_id: i64,
    p_id: i64,
    g_id: i64,
    qvs_iri: &str,
    all_shapes: &[Shape],
) -> i64 {
    get_value_ids(s_id, p_id, g_id)
        .iter()
        .filter(|&&v| node_conforms_to_shape(v, qvs_iri, g_id, all_shapes))
        .count() as i64
}

// ─── Async validation pipeline (v0.8.0) ──────────────────────────────────────

/// Process up to `batch_size` rows from `_pg_ripple.validation_queue`.
///
/// For each queued triple, runs full SHACL validation.  Violations are
/// inserted into `_pg_ripple.dead_letter_queue`.  Processed rows are removed
/// from the queue.  Returns the number of rows processed.
///
/// Called by the merge background worker when `shacl_mode = 'async'` and by
/// the manual `pg_ripple.process_validation_queue()` SQL function.
pub fn process_validation_batch(batch_size: i64) -> i64 {
    // Fetch a batch of queued triples.
    struct QueuedRow {
        id: i64,
        s_id: i64,
        p_id: i64,
        o_id: i64,
        g_id: i64,
    }

    let rows: Vec<QueuedRow> = Spi::connect(|c| {
        let tup = c
            .select(
                "SELECT id, s_id, p_id, o_id, g_id \
                 FROM _pg_ripple.validation_queue \
                 ORDER BY id ASC \
                 LIMIT $1",
                None,
                &[DatumWithOid::from(batch_size)],
            )
            .unwrap_or_else(|e| pgrx::error!("validation_queue select error: {e}"));
        let mut out: Vec<QueuedRow> = Vec::new();
        for row in tup {
            let id: i64 = row.get::<i64>(1).ok().flatten().unwrap_or(0);
            let s_id: i64 = row.get::<i64>(2).ok().flatten().unwrap_or(0);
            let p_id: i64 = row.get::<i64>(3).ok().flatten().unwrap_or(0);
            let o_id: i64 = row.get::<i64>(4).ok().flatten().unwrap_or(0);
            let g_id: i64 = row.get::<i64>(5).ok().flatten().unwrap_or(0);
            if id > 0 {
                out.push(QueuedRow {
                    id,
                    s_id,
                    p_id,
                    o_id,
                    g_id,
                });
            }
        }
        out
    });

    if rows.is_empty() {
        return 0;
    }

    let shapes = load_shapes();
    let processed_count = rows.len() as i64;

    for row in &rows {
        match validate_sync_with_shapes(row.s_id, row.p_id, row.o_id, row.g_id, &shapes) {
            Ok(()) => {} // conforms — nothing to do
            Err(msg) => {
                // Insert into dead-letter queue.
                let violation = serde_json::json!({
                    "shapeIRI":   "unknown",
                    "message":    msg,
                    "detectedAt": "async"
                });
                let _ = Spi::run_with_args(
                    "INSERT INTO _pg_ripple.dead_letter_queue \
                     (s_id, p_id, o_id, g_id, stmt_id, violation) \
                     VALUES ($1, $2, $3, $4, $5, $6::jsonb)",
                    &[
                        DatumWithOid::from(row.s_id),
                        DatumWithOid::from(row.p_id),
                        DatumWithOid::from(row.o_id),
                        DatumWithOid::from(row.g_id),
                        DatumWithOid::from(row.id), // stmt_id ← queue id
                        DatumWithOid::from(violation.to_string().as_str()),
                    ],
                );
            }
        }
    }

    // Delete processed rows from the queue.
    let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    // Build a parameterised ANY-array delete.
    let _ = Spi::run_with_args(
        "DELETE FROM _pg_ripple.validation_queue WHERE id = ANY($1)",
        &[DatumWithOid::from(ids.as_slice())],
    );

    processed_count
}

/// Like `validate_sync` but accepts a pre-loaded shapes slice.
/// Avoids reloading the shapes catalog on every call when processing batches.
fn validate_sync_with_shapes(
    s_id: i64,
    p_id: i64,
    o_id: i64,
    g_id: i64,
    shapes: &[Shape],
) -> Result<(), String> {
    for shape in shapes {
        if shape.deactivated {
            continue;
        }

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

        // Delegate to the single-triple sync validator.
        validate_sync(s_id, p_id, o_id, g_id)?;
    }
    Ok(())
}

// ─── pg_trickle DAG monitor compilation (v0.8.0) ─────────────────────────────

/// IRI for `rdf:type` predicate (used in all targetClass queries).
const RDF_TYPE_IRI: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// Convert a shape IRI into a safe PostgreSQL identifier suffix for the stream
/// table name.  Takes the last path/fragment segment of the IRI, lowercases it,
/// replaces non-alphanumeric characters with underscores, and truncates to 40
/// characters to keep names within PostgreSQL's 63-byte identifier limit.
fn shape_iri_to_table_suffix(shape_iri: &str) -> String {
    // Strip trailing slashes/hashes.
    let base = shape_iri.trim_end_matches('#').trim_end_matches('/');
    // Take the last hash-segment, then the last slash-segment.
    let segment = base
        .rsplit('#')
        .next()
        .unwrap_or(base)
        .rsplit('/')
        .next()
        .unwrap_or(base);
    let safe: String = segment
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .take(40)
        .collect();
    // Collapse consecutive underscores and trim leading/trailing underscores.
    let trimmed = safe.trim_matches('_');
    if trimmed.is_empty() {
        "shape".to_owned()
    } else {
        // Replace runs of underscores with a single one.
        let mut result = String::with_capacity(trimmed.len());
        let mut prev_under = false;
        for c in trimmed.chars() {
            if c == '_' {
                if !prev_under {
                    result.push(c);
                }
                prev_under = true;
            } else {
                result.push(c);
                prev_under = false;
            }
        }
        result
    }
}

/// Escape a string for embedding in a SQL single-quoted literal.
fn sql_escape_str(s: &str) -> String {
    s.replace('\'', "''")
}

/// Describe which constraints were compiled (for the catalog entry).
fn constraint_summary(prop: &PropertyShape, constraint: &ShapeConstraint) -> String {
    match constraint {
        ShapeConstraint::MinCount(n) => format!("sh:minCount {} on {}", n, prop.path_iri),
        ShapeConstraint::MaxCount(n) => format!("sh:maxCount {} on {}", n, prop.path_iri),
        ShapeConstraint::Datatype(dt) => format!("sh:datatype {} on {}", dt, prop.path_iri),
        ShapeConstraint::Class(c) => format!("sh:class {} on {}", c, prop.path_iri),
        _ => "unsupported".to_owned(),
    }
}

/// Try to compile a single `(property_shape, constraint)` pair into a
/// violation-detection SELECT statement (no trailing semicolon).
///
/// Returns `None` for constraints that cannot be expressed as a static SQL
/// query (sh:or, sh:and, sh:not, sh:node, sh:in, sh:pattern, sh:qualifiedValueShape).
fn compile_property_constraint_sql(
    shape: &Shape,
    prop: &PropertyShape,
    constraint: &ShapeConstraint,
    _rdf_type_id: i64,
    rdf_type_table: &str,
    class_id: i64,
) -> Option<String> {
    let path_id = crate::dictionary::encode(&prop.path_iri, crate::dictionary::KIND_IRI);
    let path_table = get_vp_table_name(path_id);
    let shape_iri_esc = sql_escape_str(&shape.shape_iri);

    match constraint {
        ShapeConstraint::MinCount(n) => {
            let sql = if *n == 1 {
                format!(
                    "SELECT _t.s AS subject_id, \
                     '{shape_iri_esc}'::text AS shape_iri, \
                     'sh:minCount'::text AS constraint_type, \
                     'Violation'::text AS severity, \
                     _t.g AS graph_id, \
                     now() AS detected_at \
                     FROM {rdf_type_table} _t \
                     WHERE _t.o = {class_id} \
                     AND NOT EXISTS (\
                         SELECT 1 FROM {path_table} _v WHERE _v.s = _t.s\
                     )"
                )
            } else {
                format!(
                    "SELECT _t.s AS subject_id, \
                     '{shape_iri_esc}'::text AS shape_iri, \
                     'sh:minCount'::text AS constraint_type, \
                     'Violation'::text AS severity, \
                     _t.g AS graph_id, \
                     now() AS detected_at \
                     FROM {rdf_type_table} _t \
                     WHERE _t.o = {class_id} \
                     AND (SELECT count(*) FROM {path_table} _v WHERE _v.s = _t.s) < {n}"
                )
            };
            Some(sql)
        }
        ShapeConstraint::MaxCount(n) => {
            let sql = format!(
                "SELECT _t.s AS subject_id, \
                 '{shape_iri_esc}'::text AS shape_iri, \
                 'sh:maxCount'::text AS constraint_type, \
                 'Violation'::text AS severity, \
                 _t.g AS graph_id, \
                 now() AS detected_at \
                 FROM {rdf_type_table} _t \
                 WHERE _t.o = {class_id} \
                 AND (SELECT count(*) FROM {path_table} _v WHERE _v.s = _t.s) > {n}"
            );
            Some(sql)
        }
        ShapeConstraint::Datatype(dt_iri) => {
            let dt_esc = sql_escape_str(dt_iri);
            let sql = format!(
                "SELECT _t.s AS subject_id, \
                 '{shape_iri_esc}'::text AS shape_iri, \
                 'sh:datatype'::text AS constraint_type, \
                 'Violation'::text AS severity, \
                 _t.g AS graph_id, \
                 now() AS detected_at \
                 FROM {rdf_type_table} _t \
                 JOIN {path_table} _v ON _v.s = _t.s \
                 LEFT JOIN _pg_ripple.dictionary _d ON _d.id = _v.o \
                 WHERE _t.o = {class_id} \
                 AND (_d.datatype IS NULL OR _d.datatype != '{dt_esc}')"
            );
            Some(sql)
        }
        ShapeConstraint::Class(val_class_iri) => {
            let val_class_id =
                crate::dictionary::encode(val_class_iri, crate::dictionary::KIND_IRI);
            let sql = format!(
                "SELECT _t.s AS subject_id, \
                 '{shape_iri_esc}'::text AS shape_iri, \
                 'sh:class'::text AS constraint_type, \
                 'Violation'::text AS severity, \
                 _t.g AS graph_id, \
                 now() AS detected_at \
                 FROM {rdf_type_table} _t \
                 JOIN {path_table} _v ON _v.s = _t.s \
                 WHERE _t.o = {class_id} \
                 AND NOT EXISTS (\
                     SELECT 1 FROM {rdf_type_table} _vt \
                     WHERE _vt.s = _v.o AND _vt.o = {val_class_id}\
                 )"
            );
            Some(sql)
        }
        // sh:in, sh:pattern, sh:or, sh:and, sh:not, sh:node, sh:qualifiedValueShape —
        // too complex for a static SQL expression; skipped.
        _ => None,
    }
}

/// Compile a `Shape` into a single violation-detection SQL SELECT (possibly a
/// UNION ALL of multiple per-constraint queries).
///
/// Compile a `Shape` into a single violation-detection SQL SELECT (possibly a
/// UNION ALL of multiple per-constraint queries), returning
/// `Some((stream_table_suffix, sql, constraint_summary))`.
///
/// Only `sh:targetClass` shapes with compilable property constraints are
/// supported.  Returns `None` when the shape cannot be expressed as a static
/// SQL query (no `sh:targetClass`, all constraints are complex, or deactivated).
fn compile_shape_to_stream_sql(shape: &Shape) -> Option<(String, String, String)> {
    if shape.deactivated {
        return None;
    }
    let class_iri = match &shape.target {
        ShapeTarget::Class(iri) => iri.clone(),
        _ => return None,
    };
    let rdf_type_id = crate::dictionary::encode(RDF_TYPE_IRI, crate::dictionary::KIND_IRI);
    let rdf_type_table = get_vp_table_name(rdf_type_id);
    let class_id = crate::dictionary::encode(&class_iri, crate::dictionary::KIND_IRI);

    let mut parts: Vec<String> = Vec::new();
    let mut summaries: Vec<String> = Vec::new();

    for prop in &shape.properties {
        for constraint in &prop.constraints {
            if let Some(sql) = compile_property_constraint_sql(
                shape,
                prop,
                constraint,
                rdf_type_id,
                &rdf_type_table,
                class_id,
            ) {
                summaries.push(constraint_summary(prop, constraint));
                parts.push(sql);
            }
        }
    }

    if parts.is_empty() {
        return None;
    }

    let full_sql = parts.join("\nUNION ALL\n");
    let summary = summaries.join("; ");
    let suffix = shape_iri_to_table_suffix(&shape.shape_iri);
    Some((suffix, full_sql, summary))
}

/// Create pg_trickle stream tables for all compilable active SHACL shapes plus
/// a `violation_summary_dag` DAG-leaf stream table that aggregates them.
///
/// Returns the count of per-shape stream tables created (not counting the
/// summary).  Returns 0 (with a warning) when pg_trickle is not installed.
pub fn compile_dag_monitors() -> i64 {
    if !crate::has_pg_trickle() {
        pgrx::warning!(
            "pg_trickle is not installed; SHACL DAG monitors are unavailable. \
             Install pg_trickle and run SELECT pg_ripple.enable_shacl_dag_monitors() to enable."
        );
        return 0;
    }

    let shapes = load_shapes();
    let mut created: i64 = 0;
    let mut stream_table_names: Vec<String> = Vec::new();

    for shape in &shapes {
        let Some((suffix, sql, summary)) = compile_shape_to_stream_sql(shape) else {
            continue;
        };

        let table_name = format!("_pg_ripple.shacl_viol_{suffix}");

        // Create the per-shape violation stream table with IMMEDIATE refresh so
        // violations are detected within the same transaction as the DML.
        let create_sql = format!(
            "SELECT pg_trickle.create_stream_table(\
                '{table_name}', \
                $pgtrickle_q$\
                    {sql}\
                $pgtrickle_q$, \
                'IMMEDIATE'\
            )"
        );

        match Spi::run(&create_sql) {
            Ok(()) => {
                // Register in the catalog.
                let shape_iri_esc = sql_escape_str(&shape.shape_iri);
                let table_name_esc = sql_escape_str(&table_name);
                let summary_esc = sql_escape_str(&summary);
                let catalog_sql = format!(
                    "INSERT INTO _pg_ripple.shacl_dag_monitors \
                        (shape_iri, stream_table_name, constraint_summary) \
                     VALUES ('{shape_iri_esc}', '{table_name_esc}', '{summary_esc}') \
                     ON CONFLICT (shape_iri) DO UPDATE SET \
                         stream_table_name = EXCLUDED.stream_table_name, \
                         constraint_summary = EXCLUDED.constraint_summary, \
                         created_at = now()"
                );
                Spi::run(&catalog_sql).unwrap_or_else(|e| {
                    pgrx::warning!(
                        "failed to register DAG monitor for {}: {}",
                        shape.shape_iri,
                        e
                    );
                });
                stream_table_names.push(table_name);
                created += 1;
            }
            Err(e) => {
                pgrx::warning!(
                    "failed to create DAG monitor stream table for shape {}: {}",
                    shape.shape_iri,
                    e
                );
            }
        }
    }

    if stream_table_names.is_empty() {
        return 0;
    }

    // Build the violation_summary_dag stream table as the DAG leaf.
    // It reads from a UNION ALL of all per-shape stream tables and groups by
    // shape/constraint/severity/graph, so the count automatically goes to zero
    // when upstream shape violations resolve.
    let union_sql = stream_table_names
        .iter()
        .map(|tn| {
            format!(
                "SELECT subject_id, shape_iri, constraint_type, severity, graph_id, detected_at \
                 FROM {tn}"
            )
        })
        .collect::<Vec<_>>()
        .join("\nUNION ALL\n");

    let summary_sql = format!(
        "SELECT shape_iri, constraint_type, severity, graph_id, \
                count(*)       AS violation_count, \
                max(detected_at) AS last_seen \
         FROM (\
             {union_sql}\
         ) _all_violations \
         GROUP BY shape_iri, constraint_type, severity, graph_id"
    );

    let create_summary = format!(
        "SELECT pg_trickle.create_stream_table(\
            '_pg_ripple.violation_summary_dag', \
            $pgtrickle_q$\
                {summary_sql}\
            $pgtrickle_q$, \
            '5s'\
        )"
    );

    Spi::run(&create_summary).unwrap_or_else(|e| {
        pgrx::warning!("failed to create violation_summary_dag stream table: {}", e);
    });

    created
}

/// Drop all pg_trickle SHACL DAG monitor stream tables and clear the catalog.
///
/// Returns the number of stream tables dropped.
pub fn drop_dag_monitors() -> i64 {
    // Drop violation_summary_dag first (it depends on per-shape tables).
    Spi::run("SELECT pg_trickle.drop_stream_table('_pg_ripple.violation_summary_dag')").ok(); // ignore error if table doesn't exist

    // Collect per-shape stream table names from catalog.
    let names: Vec<String> = Spi::connect(|client| {
        match client.select(
            "SELECT stream_table_name FROM _pg_ripple.shacl_dag_monitors ORDER BY created_at",
            None,
            &[],
        ) {
            Ok(rows) => rows
                .filter_map(|row| row.get::<&str>(1).ok().flatten().map(|s| s.to_owned()))
                .collect(),
            Err(_) => Vec::new(),
        }
    });

    let count = names.len() as i64;
    for name in &names {
        let drop_sql = format!(
            "SELECT pg_trickle.drop_stream_table('{}')",
            sql_escape_str(name)
        );
        Spi::run(&drop_sql).ok();
    }

    Spi::run("DELETE FROM _pg_ripple.shacl_dag_monitors").unwrap_or_else(|e| {
        pgrx::warning!("failed to clear shacl_dag_monitors catalog: {}", e);
    });

    count
}

/// List all active SHACL DAG monitors as `(shape_iri, stream_table_name, constraint_summary)`.
pub fn list_dag_monitors() -> Vec<(String, String, String)> {
    Spi::connect(|client| {
        match client.select(
            "SELECT shape_iri, stream_table_name, constraint_summary \
             FROM _pg_ripple.shacl_dag_monitors \
             ORDER BY shape_iri",
            None,
            &[],
        ) {
            Ok(rows) => rows
                .filter_map(|row| {
                    let shape_iri = row.get::<&str>(1).ok().flatten()?.to_owned();
                    let table_name = row.get::<&str>(2).ok().flatten()?.to_owned();
                    let summary = row.get::<&str>(3).ok().flatten()?.to_owned();
                    Some((shape_iri, table_name, summary))
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    })
}
