//! Safe, formal-prologue-only prefix resolution for SPARQL.

use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
};

/// Add registered declarations without inspecting the query body.
pub(crate) fn inject_registered_prefixes(query: &str) -> Cow<'_, str> {
    let mode = crate::SPARQL_PREFIX_MODE
        .get()
        .as_ref()
        .map(|v| v.to_string_lossy().into_owned())
        .unwrap_or_else(|| "strict".to_owned());
    if mode != "registered" {
        return Cow::Borrowed(query);
    }

    let local = local_prefixes(query);
    let declarations: BTreeMap<_, _> = crate::storage::list_prefixes()
        .into_iter()
        .filter(|(prefix, _)| !local.contains(prefix))
        .collect();
    if declarations.is_empty() {
        return Cow::Borrowed(query);
    }
    let mut prologue = String::new();
    for (prefix, expansion) in declarations {
        prologue.push_str("PREFIX ");
        prologue.push_str(&prefix);
        prologue.push_str(": <");
        prologue.push_str(&expansion);
        prologue.push_str(">\n");
    }
    Cow::Owned(format!("{prologue}{query}"))
}

pub(crate) fn has_local_prefix(query: &str, wanted: &str) -> bool {
    local_prefixes(query).contains(wanted)
}

/// Return the first form keyword after a formal BASE/PREFIX prologue.
pub(crate) fn first_form_keyword(query: &str) -> Option<String> {
    let bytes = query.as_bytes();
    let mut at = 0;
    loop {
        at = skip_space_comments(bytes, at)?;
        let (keyword, end) = token(bytes, at)?;
        if keyword.eq_ignore_ascii_case("base") {
            let next = skip_space_comments(bytes, end)?;
            if bytes.get(next) != Some(&b'<') {
                return Some(keyword.to_ascii_uppercase());
            }
            let close = bytes[next + 1..].iter().position(|b| *b == b'>')?;
            at = next + close + 2;
            continue;
        }
        if keyword.eq_ignore_ascii_case("prefix") {
            let label = skip_space_comments(bytes, end)?;
            let (_, label_end) = token(bytes, label)?;
            let iri = skip_space_comments(bytes, label_end)?;
            if bytes.get(iri) != Some(&b'<') {
                return Some(keyword.to_ascii_uppercase());
            }
            let close = bytes[iri + 1..].iter().position(|b| *b == b'>')?;
            at = iri + close + 2;
            continue;
        }
        return Some(keyword.to_ascii_uppercase());
    }
}

fn local_prefixes(query: &str) -> BTreeSet<String> {
    let bytes = query.as_bytes();
    let mut at = 0;
    let mut result = BTreeSet::new();
    while let Some(next) = skip_space_comments(bytes, at) {
        at = next;
        let Some((keyword, end)) = token(bytes, at) else {
            break;
        };
        if keyword.eq_ignore_ascii_case("base") {
            at = end;
            let Some(next) = skip_space_comments(bytes, at) else {
                break;
            };
            at = next;
            if bytes.get(at) != Some(&b'<') {
                break;
            }
            let Some(close) = bytes[at + 1..].iter().position(|b| *b == b'>') else {
                break;
            };
            at += close + 2;
            continue;
        }
        if !keyword.eq_ignore_ascii_case("prefix") {
            break;
        }
        let Some(next) = skip_space_comments(bytes, end) else {
            break;
        };
        at = next;
        let Some((label, label_end)) = token(bytes, at) else {
            break;
        };
        let Some(prefix) = label.strip_suffix(':') else {
            break;
        };
        if prefix.is_empty() {
            break;
        }
        let Some(next) = skip_space_comments(bytes, label_end) else {
            break;
        };
        at = next;
        if bytes.get(at) != Some(&b'<') {
            break;
        }
        let Some(close) = bytes[at + 1..].iter().position(|b| *b == b'>') else {
            break;
        };
        result.insert(prefix.to_owned());
        at += close + 2;
    }
    result
}

fn skip_space_comments(bytes: &[u8], mut at: usize) -> Option<usize> {
    loop {
        while matches!(bytes.get(at), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            at += 1;
        }
        if bytes.get(at) != Some(&b'#') {
            return Some(at);
        }
        while at < bytes.len() && bytes[at] != b'\n' {
            at += 1;
        }
    }
}

fn token(bytes: &[u8], mut at: usize) -> Option<(&str, usize)> {
    let start = at;
    while at < bytes.len() && !matches!(bytes[at], b' ' | b'\t' | b'\r' | b'\n') {
        at += 1;
    }
    std::str::from_utf8(&bytes[start..at]).ok().map(|s| (s, at))
}
