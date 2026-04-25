//! SHACL Turtle parser for pg_ripple.
//!
//! Provides a hand-rolled subset Turtle parser for SHACL shape definitions.
//! The implementation handles the common SHACL pattern without requiring a
//! full Turtle parser crate.

use super::{PropertyShape, Shape, ShapeConstraint, ShapeTarget};

/// Strip `/* ... */` block comments from a Turtle source string (M-11).
pub(super) fn strip_block_comments(s: &str) -> String {
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

/// Minimal Turtle parser for SHACL shapes.
/// Supports the common SHACL subset (one level of `[]` nesting).
pub(super) fn parse_shacl_turtle(data_raw: &str) -> Result<Vec<Shape>, String> {
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

    let mut lines: Vec<&str> = data.lines().map(|l| l.trim()).collect();
    lines.retain(|l| !l.starts_with('#') && !l.is_empty());
    let flat = lines.join(" ");
    let statements: Vec<&str> = split_turtle_statements(&flat);

    for stmt in &statements {
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }
        if stmt.starts_with("@prefix") || stmt.to_lowercase().starts_with("prefix") {
            parse_prefix_directive(stmt, &mut prefixes)?;
            continue;
        }
        if let Some(shape) = parse_shape_statement(stmt, &prefixes)? {
            shapes.push(shape);
        }
    }

    Ok(shapes)
}

/// Split `s` on `;` at bracket-depth 0.
pub(super) fn split_on_semicolon_top_level(s: &str) -> Vec<&str> {
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

/// Split a flattened Turtle string on `.` boundaries, respecting literals and brackets.
pub(super) fn split_turtle_statements(flat: &str) -> Vec<&str> {
    let mut result: Vec<&str> = Vec::new();
    let bytes = flat.as_bytes();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut in_iri = false;
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

/// Expand a CURIE or bracketed IRI to a full IRI, handling typed literals.
pub(super) fn expand_literal_or_iri(
    token: &str,
    prefixes: &std::collections::HashMap<String, String>,
) -> Result<String, String> {
    let token = token.trim();
    if token.starts_with('"') || token.starts_with('\'') {
        if let Some(caret_pos) = token.rfind("^^") {
            let lexical_part = &token[..caret_pos + 2];
            let datatype = token[caret_pos + 2..].trim();
            let dt_iri = expand_iri(datatype, prefixes)?;
            return Ok(format!("{lexical_part}<{dt_iri}>"));
        }
        return Ok(token.to_owned());
    }
    expand_iri(token, prefixes)
}

/// Expand a CURIE (`prefix:local`) or `<IRI>` to a full IRI string.
pub(super) fn expand_iri(
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
    Ok(token.to_owned())
}

/// Parse `@prefix p: <ns> .` or `PREFIX p: <ns>`.
pub(super) fn parse_prefix_directive(
    stmt: &str,
    prefixes: &mut std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let tokens: Vec<&str> = stmt.split_whitespace().collect();
    if tokens.len() < 3 {
        return Err(format!("malformed prefix directive: '{stmt}'"));
    }
    let prefix_token = tokens[1];
    let iri_token = tokens[2];

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
pub(super) fn parse_shape_statement(
    stmt: &str,
    prefixes: &std::collections::HashMap<String, String>,
) -> Result<Option<Shape>, String> {
    let stmt = stmt.trim();
    let (subject_token, rest) = match stmt.find(char::is_whitespace) {
        Some(i) => (stmt[..i].trim(), stmt[i..].trim()),
        None => return Ok(None),
    };

    let shape_iri = expand_iri(subject_token, prefixes)?;
    let po_pairs: Vec<&str> = split_on_semicolon_top_level(rest);

    let mut is_shape = false;
    let mut target = ShapeTarget::None;
    let mut constraints: Vec<ShapeConstraint> = Vec::new();
    let mut properties: Vec<PropertyShape> = Vec::new();
    let mut deactivated = false;
    let mut closed = false;
    let mut ignored_properties: Vec<String> = Vec::new();

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
                if let Some(ps) = parse_property_shape(obj_rest.trim(), prefixes)? {
                    is_shape = true;
                    properties.push(ps);
                }
            }
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
            "http://www.w3.org/ns/shacl#equals" => {
                let iri = expand_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::Equals(iri));
            }
            "http://www.w3.org/ns/shacl#disjoint" => {
                let iri = expand_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::Disjoint(iri));
            }
            "http://www.w3.org/ns/shacl#minLength" => {
                let n: i64 = obj_rest
                    .trim()
                    .parse()
                    .map_err(|_| format!("sh:minLength value is not an integer: '{obj_rest}'"))?;
                constraints.push(ShapeConstraint::MinLength(n));
            }
            "http://www.w3.org/ns/shacl#maxLength" => {
                let n: i64 = obj_rest
                    .trim()
                    .parse()
                    .map_err(|_| format!("sh:maxLength value is not an integer: '{obj_rest}'"))?;
                constraints.push(ShapeConstraint::MaxLength(n));
            }
            "http://www.w3.org/ns/shacl#xone" => {
                let shape_iris = parse_list_values(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::Xone(shape_iris));
            }
            "http://www.w3.org/ns/shacl#minExclusive" => {
                let val = expand_literal_or_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::MinExclusive(val));
            }
            "http://www.w3.org/ns/shacl#maxExclusive" => {
                let val = expand_literal_or_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::MaxExclusive(val));
            }
            "http://www.w3.org/ns/shacl#minInclusive" => {
                let val = expand_literal_or_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::MinInclusive(val));
            }
            "http://www.w3.org/ns/shacl#maxInclusive" => {
                let val = expand_literal_or_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::MaxInclusive(val));
            }
            "http://www.w3.org/ns/shacl#sparql" => {
                let sparql_query = obj_rest.trim().to_owned();
                constraints.push(ShapeConstraint::SparqlConstraint {
                    sparql_query,
                    message: None,
                });
            }
            _ => {}
        }
    }

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

/// Parse a property shape from a `[ sh:path ... ; ... ]` block.
pub(super) fn parse_property_shape(
    block: &str,
    prefixes: &std::collections::HashMap<String, String>,
) -> Result<Option<PropertyShape>, String> {
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
            "http://www.w3.org/ns/shacl#equals" => {
                let iri = expand_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::Equals(iri));
            }
            "http://www.w3.org/ns/shacl#disjoint" => {
                let iri = expand_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::Disjoint(iri));
            }
            "http://www.w3.org/ns/shacl#minLength" => {
                let n: i64 = obj_rest
                    .trim()
                    .parse()
                    .map_err(|_| format!("sh:minLength value is not an integer: '{obj_rest}'"))?;
                constraints.push(ShapeConstraint::MinLength(n));
            }
            "http://www.w3.org/ns/shacl#maxLength" => {
                let n: i64 = obj_rest
                    .trim()
                    .parse()
                    .map_err(|_| format!("sh:maxLength value is not an integer: '{obj_rest}'"))?;
                constraints.push(ShapeConstraint::MaxLength(n));
            }
            "http://www.w3.org/ns/shacl#xone" => {
                let shape_iris = parse_list_values(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::Xone(shape_iris));
            }
            "http://www.w3.org/ns/shacl#minExclusive" => {
                let val = expand_literal_or_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::MinExclusive(val));
            }
            "http://www.w3.org/ns/shacl#maxExclusive" => {
                let val = expand_literal_or_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::MaxExclusive(val));
            }
            "http://www.w3.org/ns/shacl#minInclusive" => {
                let val = expand_literal_or_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::MinInclusive(val));
            }
            "http://www.w3.org/ns/shacl#maxInclusive" => {
                let val = expand_literal_or_iri(obj_rest.trim(), prefixes)?;
                constraints.push(ShapeConstraint::MaxInclusive(val));
            }
            _ => {}
        }
    }

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
pub(super) fn extract_string_literal(token: &str) -> Result<String, String> {
    let token = token.trim();
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
pub(super) fn parse_list_values(
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
pub(super) fn uuid_short() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{ns:08x}")
}
