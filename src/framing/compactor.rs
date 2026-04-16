//! `@context` compaction — apply prefix substitution to the framed output tree.
//!
//! This is a lightweight prefix-substitution pass, not a full JSON-LD
//! compaction algorithm. Extracts `prefix → IRI` mappings from the frame's
//! `@context` block and replaces full IRI strings with their compact forms.

use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Apply `@context` compaction to a framed JSON-LD document.
///
/// Extracts prefix mappings from `frame["@context"]`, walks the output tree,
/// and substitutes full IRIs with their compact prefixed forms.
/// Injects the `@context` block as the first key of the returned document.
pub fn compact(mut doc: Value, frame: &Value) -> Value {
    let context = extract_context(frame);
    let prefixes = build_prefix_map(&context);

    if prefixes.is_empty() && context.is_null() {
        // No compaction to apply.
        return doc;
    }

    // Apply prefix substitution throughout the tree.
    if !prefixes.is_empty() {
        compact_value(&mut doc, &prefixes);
    }

    // Inject @context as the first key of the output.
    if !context.is_null() {
        match doc {
            Value::Object(ref mut obj) => {
                // Prepend @context by rebuilding the map with it first.
                let mut new_map = Map::new();
                new_map.insert("@context".to_owned(), context);
                for (k, v) in obj.iter() {
                    new_map.insert(k.clone(), v.clone());
                }
                return Value::Object(new_map);
            }
            _ => {
                let mut wrapper = Map::new();
                wrapper.insert("@context".to_owned(), context);
                wrapper.insert("@graph".to_owned(), doc);
                return Value::Object(wrapper);
            }
        }
    }

    doc
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Extract the `@context` value from a frame, or `Value::Null` if absent.
fn extract_context(frame: &Value) -> Value {
    frame
        .as_object()
        .and_then(|o| o.get("@context"))
        .cloned()
        .unwrap_or(Value::Null)
}

/// Build a `prefix → base_iri` map from a `@context` value.
///
/// Handles both object form `{"ex": "http://example.org/"}` and
/// array form `[{"ex": "http://example.org/"}, ...]`.
fn build_prefix_map(context: &Value) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();

    match context {
        Value::Object(obj) => {
            for (k, v) in obj {
                if k.starts_with('@') {
                    continue;
                }
                if let Some(iri) = v.as_str() {
                    map.insert(k.clone(), iri.to_owned());
                } else if let Some(iri) = v
                    .as_object()
                    .and_then(|o| o.get("@id"))
                    .and_then(Value::as_str)
                {
                    map.insert(k.clone(), iri.to_owned());
                }
            }
        }
        Value::Array(arr) => {
            for item in arr {
                let sub = build_prefix_map(item);
                map.extend(sub);
            }
        }
        _ => {}
    }

    map
}

/// Compact a full IRI string to its shortest matching prefixed form.
///
/// Returns the compact form, or the original IRI if no prefix matches.
fn compact_iri<'a>(iri: &'a str, prefixes: &BTreeMap<String, String>) -> std::borrow::Cow<'a, str> {
    // Try longest base IRI first to avoid short-prefix false matches.
    let mut best: Option<(&str, &str)> = None;
    for (prefix, base) in prefixes {
        if iri.starts_with(base.as_str()) {
            let local = &iri[base.len()..];
            if !local.contains('/') && !local.contains('#') {
                // Prefer longer base match.
                if best.is_none_or(|(_, b)| b.len() < base.len()) {
                    best = Some((prefix.as_str(), base.as_str()));
                }
            }
        }
    }
    if let Some((prefix, base)) = best {
        let local = &iri[base.len()..];
        std::borrow::Cow::Owned(format!("{prefix}:{local}"))
    } else {
        std::borrow::Cow::Borrowed(iri)
    }
}

/// Recursively walk the JSON tree and apply prefix compaction to IRI strings.
fn compact_value(val: &mut Value, prefixes: &BTreeMap<String, String>) {
    match val {
        Value::Object(obj) => {
            for (key, child) in obj.iter_mut() {
                if key == "@id" || key == "@type" {
                    if let Value::String(s) = child {
                        *s = compact_iri(s, prefixes).into_owned();
                    } else if key == "@type"
                        && let Value::Array(arr) = child
                    {
                        for item in arr.iter_mut() {
                            if let Value::String(s) = item {
                                *s = compact_iri(s, prefixes).into_owned();
                            }
                        }
                    }
                } else {
                    compact_value(child, prefixes);
                }
            }
        }
        Value::Array(arr) => {
            for item in arr.iter_mut() {
                compact_value(item, prefixes);
            }
        }
        _ => {}
    }
}
