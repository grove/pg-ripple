//! sh:pattern, sh:languageIn, sh:uniqueLang, sh:minLength, sh:maxLength constraint checkers.

use pgrx::prelude::*;

use super::{ConstraintArgs, Violation, get_language_tag, get_value_ids};

/// Check `sh:pattern regex` — every value's lexical form must match the regex.
pub(crate) fn check_pattern(regex: &str, args: &ConstraintArgs, violations: &mut Vec<Violation>) {
    let value_ids = get_value_ids(args.focus, args.path_id, args.graph_id);
    for v_id in value_ids {
        let lexical = crate::dictionary::decode(v_id).unwrap_or_default();
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
                pgrx::datum::DatumWithOid::from(lexical.as_str()),
                pgrx::datum::DatumWithOid::from(regex),
            ],
        )
        .unwrap_or(None);
        if !matches.unwrap_or(false) {
            let focus_iri = crate::dictionary::decode(args.focus)
                .unwrap_or_else(|| format!("_id_{}", args.focus));
            violations.push(Violation {
                focus_node: focus_iri,
                shape_iri: args.shape_iri.to_owned(),
                path: Some(args.path_iri.to_owned()),
                constraint: "sh:pattern".to_owned(),
                message: format!("value '{lexical}' does not match pattern /{regex}/"),
                severity: "Violation".to_owned(),
                sh_value: None,
                sh_source_constraint_component: None,
            });
        }
    }
}

/// Check `sh:languageIn [tags]` — every literal value must have a language tag in the list.
pub(crate) fn check_language_in(
    allowed_tags: &[String],
    args: &ConstraintArgs,
    violations: &mut Vec<Violation>,
) {
    let value_ids = get_value_ids(args.focus, args.path_id, args.graph_id);
    for v_id in value_ids {
        let lang = get_language_tag(v_id);
        let ok = match &lang {
            Some(tag) => allowed_tags.iter().any(|t| t.eq_ignore_ascii_case(tag)),
            None => false,
        };
        if !ok {
            let focus_iri = crate::dictionary::decode(args.focus)
                .unwrap_or_else(|| format!("_id_{}", args.focus));
            violations.push(Violation {
                focus_node: focus_iri,
                shape_iri: args.shape_iri.to_owned(),
                path: Some(args.path_iri.to_owned()),
                constraint: "sh:languageIn".to_owned(),
                message: format!(
                    "value id {v_id} has language tag {:?}, not in {:?}",
                    lang, allowed_tags
                ),
                severity: "Violation".to_owned(),
                sh_value: None,
                sh_source_constraint_component: None,
            });
        }
    }
}

/// Check `sh:uniqueLang true` — no two values along the path may share a language tag.
pub(crate) fn check_unique_lang(args: &ConstraintArgs, violations: &mut Vec<Violation>) {
    let value_ids = get_value_ids(args.focus, args.path_id, args.graph_id);
    let mut seen_tags: std::collections::HashSet<String> = std::collections::HashSet::new();
    for v_id in &value_ids {
        if let Some(tag) = get_language_tag(*v_id)
            && !seen_tags.insert(tag.clone())
        {
            let focus_iri = crate::dictionary::decode(args.focus)
                .unwrap_or_else(|| format!("_id_{}", args.focus));
            violations.push(Violation {
                focus_node: focus_iri,
                shape_iri: args.shape_iri.to_owned(),
                path: Some(args.path_iri.to_owned()),
                constraint: "sh:uniqueLang".to_owned(),
                message: format!("duplicate language tag '{tag}' among values"),
                severity: "Violation".to_owned(),
                sh_value: None,
                sh_source_constraint_component: None,
            });
            break; // Report once per focus node.
        }
    }
}

/// Extract the lexical form of a value node (strip surrounding quotes for string literals,
/// strip language tags and datatype suffixes).
fn lexical_form(raw: &str) -> &str {
    // Handle `"..."`, `"..."@lang`, `"..."^^<type>`, `"..."^^prefix:local`.
    if let Some(inner) = raw.strip_prefix('"') {
        // Find the closing quote.
        if let Some(end) = inner.find('"') {
            return &inner[..end];
        }
    }
    raw
}

/// Check `sh:minLength n` — every value's lexical form must have length >= n.
pub(crate) fn check_min_length(min: i64, args: &ConstraintArgs, violations: &mut Vec<Violation>) {
    let value_ids = get_value_ids(args.focus, args.path_id, args.graph_id);
    for v_id in value_ids {
        let raw = crate::dictionary::decode(v_id).unwrap_or_default();
        let lex = lexical_form(&raw);
        // char count (Unicode code points)
        let len = lex.chars().count() as i64;
        if len < min {
            let focus_iri = crate::dictionary::decode(args.focus)
                .unwrap_or_else(|| format!("_id_{}", args.focus));
            violations.push(Violation {
                focus_node: focus_iri,
                shape_iri: args.shape_iri.to_owned(),
                path: Some(args.path_iri.to_owned()),
                constraint: "sh:minLength".to_owned(),
                message: format!("value '{lex}' has length {len}, expected at least {min}"),
                severity: "Violation".to_owned(),
                sh_value: None,
                sh_source_constraint_component: None,
            });
        }
    }
}

/// Check `sh:maxLength n` — every value's lexical form must have length <= n.
pub(crate) fn check_max_length(max: i64, args: &ConstraintArgs, violations: &mut Vec<Violation>) {
    let value_ids = get_value_ids(args.focus, args.path_id, args.graph_id);
    for v_id in value_ids {
        let raw = crate::dictionary::decode(v_id).unwrap_or_default();
        let lex = lexical_form(&raw);
        let len = lex.chars().count() as i64;
        if len > max {
            let focus_iri = crate::dictionary::decode(args.focus)
                .unwrap_or_else(|| format!("_id_{}", args.focus));
            violations.push(Violation {
                focus_node: focus_iri,
                shape_iri: args.shape_iri.to_owned(),
                path: Some(args.path_iri.to_owned()),
                constraint: "sh:maxLength".to_owned(),
                message: format!("value '{lex}' has length {len}, expected at most {max}"),
                severity: "Violation".to_owned(),
                sh_value: None,
                sh_source_constraint_component: None,
            });
        }
    }
}
