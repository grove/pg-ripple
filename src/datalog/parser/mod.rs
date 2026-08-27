//! Datalog rule parser: Turtle-flavoured Datalog syntax.

use crate::datalog::{Rule, RuleSet};

mod ast;
mod lexer;

#[cfg(test)]
use crate::datalog::{BodyLiteral, CompareOp};
#[cfg(test)]
use ast::try_parse_comparison;
#[cfg(test)]
use lexer::{split_body, tokenize_rules};

/// Parse a Datalog rule text into a `RuleSet` IR.
pub fn parse_rules(text: &str, rule_set_name: &str) -> Result<RuleSet, String> {
    let mut rules = Vec::new();
    let mut errors = Vec::new();
    let lines = lexer::tokenize_rules(text);
    for (line_num, line) in lines.iter().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match parse_rule(line) {
            Ok(rule) => rules.push(rule),
            Err(e) => errors.push(format!("line {}: {e}", line_num + 1)),
        }
    }
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }
    Ok(RuleSet {
        name: rule_set_name.to_owned(),
        rules,
    })
}

fn parse_rule(text: &str) -> Result<Rule, String> {
    // v0.87.0: extract @weight(FLOAT) annotation before parsing rule body.
    let (rule_body, weight) = extract_weight_annotation(text);
    let rule_text = rule_body.trim().to_owned() + " .";

    // Constraint rule: starts with ':-'
    if rule_body.trim_start().starts_with(":-") {
        let body_text = rule_body.trim_start()[2..].trim().to_owned();
        let body = ast::parse_body(&body_text)?;
        return Ok(Rule {
            head: None,
            body,
            rule_text,
            weight,
        });
    }

    // Normal rule: head :- body
    let sep = find_neck(rule_body)?;
    let head_text = rule_body[..sep].trim();
    let body_text = rule_body[sep + 2..].trim();

    let head = ast::parse_head(head_text)?;
    let body = ast::parse_body(body_text)?;

    Ok(Rule {
        head: Some(head),
        body,
        rule_text,
        weight,
    })
}

/// Extract `@weight(FLOAT)` annotation from a rule text, returning (body_text, weight).
///
/// v0.87.0: Supports `@weight(0.85)` anywhere after the rule body.
/// The value must be in [0.0, 1.0]; values outside this range trigger PT0301.
fn extract_weight_annotation(text: &str) -> (&str, Option<f64>) {
    if let Some(pos) = text.rfind("@weight(") {
        let annotation = &text[pos..];
        if let Some(end) = annotation.find(')') {
            let inner = &annotation[8..end]; // "8" = len("@weight(")
            match inner.trim().parse::<f64>() {
                Ok(w) if (0.0..=1.0).contains(&w) => {
                    return (&text[..pos], Some(w));
                }
                Ok(w) => {
                    pgrx::error!("rule weight must be in [0.0, 1.0]; got {} (PT0301)", w);
                }
                Err(_) => {
                    pgrx::error!(
                        "invalid @weight annotation: expected a float literal, got '{}' (PT0301)",
                        inner.trim()
                    );
                }
            }
        }
    }
    (text, None)
}

/// Find the position of `:-` that is not inside a literal or IRI.
pub(super) fn find_neck(text: &str) -> Result<usize, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut in_literal = false;
    let mut in_iri = false;
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '"' => in_literal = !in_literal,
            '<' if !in_literal => in_iri = true,
            '>' if !in_literal => in_iri = false,
            '-' if !in_literal && !in_iri && i > 0 && chars[i - 1] == ':' => {
                return Ok(i - 1);
            }
            _ => {}
        }
        i += 1;
    }
    Err(format!("missing ':-' in rule: {text}"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[path = "../parser_tests.rs"]
mod tests;
