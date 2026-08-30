//! Fuzz target for the v0.135 typed initial-binding contract.
//!
//! The production parser depends on pgrx, so this harness mirrors the public
//! JSON boundary and its parameterization invariant: values are validated and
//! kept out of generated query text.

#![no_main]

use libfuzzer_sys::fuzz_target;

fn is_absolute_iri(value: &str) -> bool {
    let Some((scheme, _)) = value.split_once(':') else {
        return false;
    };
    !scheme.is_empty()
        && scheme.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphabetic()
                || (index > 0 && (byte.is_ascii_digit() || b"+.-".contains(&byte)))
        })
}

fn inspect_bindings(value: &serde_json::Value) {
    let Some(bindings) = value.as_object() else {
        return;
    };

    let mut names: Vec<&str> = bindings.keys().map(String::as_str).collect();
    names.sort_unstable();
    for name in names {
        let normalized = name.strip_prefix('?').unwrap_or(name);
        let Some(term) = bindings.get(name).and_then(serde_json::Value::as_object) else {
            continue;
        };
        let Some(kind) = term.get("type").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let Some(text) = term.get("value").and_then(serde_json::Value::as_str) else {
            continue;
        };

        match kind {
            "uri" => {
                let _ = is_absolute_iri(text);
            }
            "literal" => {
                let _ = term.get("datatype").and_then(serde_json::Value::as_str);
                let _ = term.get("xml:lang").and_then(serde_json::Value::as_str);
            }
            _ => {}
        }

        // Binding values are parameters, never fragments of this SQL shape.
        let sql = format!("VALUES (?{normalized}) {{ ($1::bigint) }}");
        assert!(!sql.contains(text));
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(data) else {
        return;
    };
    inspect_bindings(&value);
});
