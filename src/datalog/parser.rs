//! Datalog rule parser: Turtle-flavoured Datalog syntax.
//!
//! # Syntax
//!
//! ```text
//! RuleSet       ::= (Rule | Comment)*
//! Rule          ::= Head ':-' Body '.' | ':-' Body '.'
//! Head          ::= [GraphPattern] TriplePattern
//! Body          ::= Literal (',' Literal)*
//! Literal       ::= 'NOT'? [GraphPattern] TriplePattern
//!                 | CompareExpr
//!                 | AssignExpr
//!                 | StringBuiltin
//! GraphPattern  ::= 'GRAPH' GraphTerm '{'  '}'
//! GraphTerm     ::= Variable | PrefixedIRI | FullIRI
//! TriplePattern ::= Term Term Term
//! Term          ::= Variable | PrefixedIRI | FullIRI | RDFLiteral
//! Variable      ::= '?' [a-zA-Z_][a-zA-Z0-9_]*
//! ```

use crate::datalog::{ArithOp, Atom, BodyLiteral, CompareOp, Rule, RuleSet, StringBuiltin, Term};

// ─── Public API ──────────────────────────────────────────────────────────────

/// Parse a Datalog rule text into a `RuleSet` IR.
///
/// `rule_set_name` is used for error messages and catalog storage.
pub fn parse_rules(text: &str, rule_set_name: &str) -> Result<RuleSet, String> {
    let mut rules = Vec::new();
    let mut errors = Vec::new();

    // Pre-register standard prefixes so rules can use them without declaring.
    // Additional prefixes are resolved via the _pg_ripple.prefixes table.
    let lines = tokenize_rules(text);

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

// ─── Tokenizer: split on rule-ending '.' ─────────────────────────────────────

fn tokenize_rules(text: &str) -> Vec<String> {
    let mut rules = Vec::new();
    let mut current = String::new();
    let mut in_literal = false;
    let mut in_iri = false;

    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' if !in_iri => {
                in_literal = !in_literal;
                current.push(c);
            }
            '<' if !in_literal => {
                in_iri = true;
                current.push(c);
            }
            '>' if !in_literal && in_iri => {
                in_iri = false;
                current.push(c);
            }
            '.' if !in_literal && !in_iri => {
                let trimmed = current.trim().to_owned();
                if !trimmed.is_empty() {
                    rules.push(trimmed);
                }
                current.clear();
            }
            '#' if !in_literal && !in_iri => {
                // Line comment — skip until end of line.
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            _ => current.push(c),
        }
        i += 1;
    }
    let trimmed = current.trim().to_owned();
    if !trimmed.is_empty() {
        rules.push(trimmed);
    }
    rules
}

// ─── Rule parser ─────────────────────────────────────────────────────────────

/// Parse a single rule (without the trailing `.`).
fn parse_rule(text: &str) -> Result<Rule, String> {
    let rule_text = text.trim().to_owned() + " .";

    // Constraint rule: starts with ':-'
    if text.trim_start().starts_with(":-") {
        let body_text = text.trim_start()[2..].trim().to_owned();
        let body = parse_body(&body_text)?;
        return Ok(Rule {
            head: None,
            body,
            rule_text,
        });
    }

    // Normal rule: head :- body
    let sep = find_neck(text)?;
    let head_text = text[..sep].trim();
    let body_text = text[sep + 2..].trim();

    let head = parse_head(head_text)?;
    let body = parse_body(body_text)?;

    Ok(Rule {
        head: Some(head),
        body,
        rule_text,
    })
}

/// Find the position of `:-` that is not inside a literal or IRI.
fn find_neck(text: &str) -> Result<usize, String> {
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

/// Parse the head of a rule (single atom, optionally with GRAPH clause).
fn parse_head(text: &str) -> Result<Atom, String> {
    parse_atom(text.trim())
}

/// Parse the body: a comma-separated list of literals.
fn parse_body(text: &str) -> Result<Vec<BodyLiteral>, String> {
    let literals = split_body(text);
    let mut body = Vec::new();
    for lit in literals {
        let lit = lit.trim();
        if lit.is_empty() {
            continue;
        }
        body.push(parse_body_literal(lit)?);
    }
    Ok(body)
}

/// Split body on commas, respecting nested brackets, literals, and IRIs.
fn split_body(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut in_literal = false;
    let mut in_iri = false;

    for c in text.chars() {
        match c {
            '"' => {
                in_literal = !in_literal;
                current.push(c);
            }
            '<' if !in_literal => {
                in_iri = true;
                current.push(c);
            }
            '>' if !in_literal && in_iri => {
                in_iri = false;
                current.push(c);
            }
            '{' | '(' if !in_literal && !in_iri => {
                depth += 1;
                current.push(c);
            }
            '}' | ')' if !in_literal && !in_iri => {
                depth -= 1;
                current.push(c);
            }
            ',' if !in_literal && !in_iri && depth == 0 => {
                parts.push(current.trim().to_owned());
                current.clear();
            }
            _ => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_owned());
    }
    parts
}

/// Parse a single body literal.
fn parse_body_literal(text: &str) -> Result<BodyLiteral, String> {
    let text = text.trim();

    // NOT <atom>
    if let Some(rest) = text.strip_prefix("NOT").map(str::trim_start) {
        if rest.starts_with(|c: char| c.is_alphanumeric() || c == '<' || c == '?') {
            // Some versions may write NOT followed by space
        }
        let atom = parse_atom(rest)?;
        return Ok(BodyLiteral::Negated(atom));
    }

    // Arithmetic assign: ?z IS ?x + ?y
    if let Some(assign) = try_parse_assign(text) {
        return Ok(assign);
    }

    // Comparison: ?x OP ?y or STRLEN/REGEX builtins
    if let Some(cmp) = try_parse_comparison(text) {
        return Ok(cmp);
    }

    // String builtins: STRLEN, REGEX
    if let Some(builtin) = try_parse_string_builtin(text) {
        return Ok(builtin);
    }

    // Positive atom
    let atom = parse_atom(text)?;
    Ok(BodyLiteral::Positive(atom))
}

/// Try parsing an arithmetic assignment: `?z IS ?x + ?y` or `?z IS ?x * ?y`.
fn try_parse_assign(text: &str) -> Option<BodyLiteral> {
    // Format: ?var IS term OP term
    let tokens: Vec<&str> = text.split_whitespace().collect();
    if tokens.len() < 5 {
        return None;
    }
    if tokens[1].to_uppercase() != "IS" {
        return None;
    }
    let target_var = parse_variable(tokens[0])?;
    let lhs = parse_term_simple(tokens[2]).ok()?;
    let op = match tokens[3] {
        "+" => ArithOp::Add,
        "-" => ArithOp::Sub,
        "*" => ArithOp::Mul,
        "/" => ArithOp::Div,
        _ => return None,
    };
    let rhs = parse_term_simple(tokens[4]).ok()?;
    Some(BodyLiteral::Assign(target_var, lhs, op, rhs))
}

/// Try parsing a comparison: `?a OP ?b` or `?a OP <literal>`.
fn try_parse_comparison(text: &str) -> Option<BodyLiteral> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    if tokens.len() < 3 {
        return None;
    }
    // Detect operator in position 1
    let op = match tokens[1] {
        ">" => CompareOp::Gt,
        ">=" => CompareOp::Gte,
        "<" => CompareOp::Lt,
        "<=" => CompareOp::Lte,
        "=" => CompareOp::Eq,
        "!=" => CompareOp::Neq,
        _ => return None,
    };
    let lhs = parse_term_simple(tokens[0]).ok()?;
    let rhs = parse_term_simple(tokens[2]).ok()?;
    Some(BodyLiteral::Compare(lhs, op, rhs))
}

/// Try parsing a string builtin: `STRLEN(?s) > ?n` or `REGEX(?s, "pattern")`.
fn try_parse_string_builtin(text: &str) -> Option<BodyLiteral> {
    let upper = text.to_uppercase();
    if upper.starts_with("STRLEN(") {
        // STRLEN(?s) > ?n
        let inner_end = text.find(')')?;
        let inner = &text[7..inner_end];
        let term = parse_term_simple(inner.trim()).ok()?;
        let rest = text[inner_end + 1..].trim();
        let parts: Vec<&str> = rest.splitn(2, char::is_whitespace).collect();
        if parts.len() < 2 {
            return None;
        }
        let op = match parts[0] {
            ">" => CompareOp::Gt,
            ">=" => CompareOp::Gte,
            "<" => CompareOp::Lt,
            "<=" => CompareOp::Lte,
            "=" => CompareOp::Eq,
            "!=" => CompareOp::Neq,
            _ => return None,
        };
        let rhs = parse_term_simple(parts[1].trim()).ok()?;
        return Some(BodyLiteral::StringBuiltin(StringBuiltin::Strlen(
            term, op, rhs,
        )));
    }
    if upper.starts_with("REGEX(") {
        // REGEX(?s, "pattern")
        let inner_end = text.rfind(')')?;
        let inner = &text[6..inner_end];
        let parts = split_body(inner);
        if parts.len() < 2 {
            return None;
        }
        let var_term = parse_term_simple(parts[0].trim()).ok()?;
        let pattern = parts[1].trim().trim_matches('"').to_owned();
        return Some(BodyLiteral::StringBuiltin(StringBuiltin::Regex(
            var_term, pattern,
        )));
    }
    None
}

/// Parse a variable name from `?var` or `?_` (wildcard).
fn parse_variable(text: &str) -> Option<String> {
    text.strip_prefix('?').map(|s| s.to_owned())
}

/// Parse a triple atom with optional GRAPH clause.
///
/// Forms:
/// - `<s> <p> <o>`
/// - `?s <p> ?o`
/// - `GRAPH <g> { <s> <p> <o> }`
/// - `GRAPH ?g { <s> <p> <o> }`
fn parse_atom(text: &str) -> Result<Atom, String> {
    let text = text.trim();

    let upper = text.to_uppercase();
    if upper.starts_with("GRAPH") {
        let rest = text[5..].trim();
        // Find the graph term (up to the '{')
        let brace = rest
            .find('{')
            .ok_or_else(|| format!("missing '{{' in GRAPH pattern: {text}"))?;
        let graph_term_str = rest[..brace].trim();
        let inner = rest[brace + 1..].trim();
        let inner = inner
            .strip_suffix('}')
            .ok_or_else(|| format!("missing '}}' in GRAPH pattern: {text}"))?
            .trim();

        let g = parse_term(graph_term_str)?;
        let (s, p, o) = parse_triple_terms(inner)?;
        return Ok(Atom { s, p, o, g });
    }

    let (s, p, o) = parse_triple_terms(text)?;
    Ok(Atom {
        s,
        p,
        o,
        g: Term::DefaultGraph,
    })
}

/// Parse three whitespace-separated terms for a triple pattern.
fn parse_triple_terms(text: &str) -> Result<(Term, Term, Term), String> {
    let tokens = tokenize_terms(text);
    if tokens.len() < 3 {
        return Err(format!(
            "expected 3 terms in triple pattern, got {}: {text}",
            tokens.len()
        ));
    }
    let s = parse_term(&tokens[0])?;
    let p = parse_term(&tokens[1])?;
    // Object may be a multi-token literal; join remaining tokens.
    let o_text = if tokens.len() == 3 {
        tokens[2].clone()
    } else {
        tokens[2..].join(" ")
    };
    let o = parse_term(&o_text)?;
    Ok((s, p, o))
}

/// Tokenize a term list, respecting IRIs and literals.
fn tokenize_terms(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_literal = false;
    let mut in_iri = false;
    let mut in_quoted = false; // << >> quoted triple

    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => {
                in_literal = !in_literal;
                current.push(c);
            }
            '<' if !in_literal => {
                // Check for <<
                if i + 1 < chars.len() && chars[i + 1] == '<' {
                    in_quoted = true;
                    current.push(c);
                    current.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                in_iri = true;
                current.push(c);
            }
            '>' if !in_literal && in_quoted => {
                // Check for >>
                if i + 1 < chars.len() && chars[i + 1] == '>' {
                    in_quoted = false;
                    current.push(c);
                    current.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                current.push(c);
            }
            '>' if !in_literal && in_iri => {
                in_iri = false;
                current.push(c);
            }
            ' ' | '\t' | '\n' if !in_literal && !in_iri && !in_quoted => {
                if !current.is_empty() {
                    tokens.push(current.trim().to_owned());
                    current.clear();
                }
            }
            _ => current.push(c),
        }
        i += 1;
    }
    if !current.trim().is_empty() {
        tokens.push(current.trim().to_owned());
    }
    tokens
}

/// Parse a single term (simple form without GRAPH context).
fn parse_term_simple(text: &str) -> Result<Term, String> {
    parse_term(text)
}

/// Parse a single RDF term.
fn parse_term(text: &str) -> Result<Term, String> {
    let text = text.trim();

    // Variable
    if let Some(name) = text.strip_prefix('?') {
        if name == "_" {
            return Ok(Term::Wildcard);
        }
        return Ok(Term::Var(name.to_owned()));
    }

    // Full IRI <…>
    if text.starts_with('<') && text.ends_with('>') {
        let iri = &text[1..text.len() - 1];
        return Ok(Term::Const(crate::datalog::encode_iri(iri)));
    }

    // Quoted triple << s p o >>
    if text.starts_with("<<") && text.ends_with(">>") {
        let inner = &text[2..text.len() - 2].trim();
        let (s, p, o) = parse_triple_terms(inner)?;
        let s_id = term_to_const(&s)?;
        let p_id = term_to_const(&p)?;
        let o_id = term_to_const(&o)?;
        let id = crate::dictionary::encode_quoted_triple(s_id, p_id, o_id);
        return Ok(Term::Const(id));
    }

    // Typed literal "value"^^<datatype>
    if text.starts_with('"')
        && let Some((val, rest)) = split_literal(text)
    {
        if let Some(dt_str) = rest.strip_prefix("^^") {
            let dt = dt_str.trim().trim_start_matches('<').trim_end_matches('>');
            let dt_resolved = crate::datalog::resolve_prefix(dt);
            let id = crate::dictionary::encode_typed_literal(&val, &dt_resolved);
            return Ok(Term::Const(id));
        }
        if let Some(lang) = rest.strip_prefix('@') {
            let id = crate::dictionary::encode_lang_literal(&val, lang);
            return Ok(Term::Const(id));
        }
        // Plain literal
        let id = crate::dictionary::encode(&val, crate::dictionary::KIND_LITERAL);
        return Ok(Term::Const(id));
    }

    // Blank node _:name
    if let Some(rest) = text.strip_prefix("_:") {
        let id = crate::dictionary::encode(rest, crate::dictionary::KIND_BLANK);
        return Ok(Term::Const(id));
    }

    // Bare numeric literal (integer or decimal): 18, -3, 3.14
    if text
        .chars()
        .next()
        .map(|c| c.is_ascii_digit() || c == '-' || c == '+')
        .unwrap_or(false)
    {
        let looks_numeric = text
            .trim_start_matches(['+', '-'])
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.');
        if looks_numeric {
            let dt = if text.contains('.') {
                "http://www.w3.org/2001/XMLSchema#decimal"
            } else {
                "http://www.w3.org/2001/XMLSchema#integer"
            };
            let id = crate::dictionary::encode_typed_literal(text, dt);
            return Ok(Term::Const(id));
        }
    }

    // Prefixed IRI: prefix:local — resolve via prefix registry
    if text.contains(':') && !text.contains(' ') {
        let iri = crate::datalog::resolve_prefix(text);
        if iri != text {
            return Ok(Term::Const(crate::datalog::encode_iri(&iri)));
        }
        // Try to encode as-is (may be a full IRI without angle brackets)
        return Ok(Term::Const(crate::datalog::encode_iri(&iri)));
    }

    Err(format!("unrecognized term: {text}"))
}

/// Split a quoted literal string from its type annotation.
/// Returns `(unescaped_value, rest_after_closing_quote)`.
fn split_literal(text: &str) -> Option<(String, &str)> {
    let bytes = text.as_bytes();
    if bytes[0] != b'"' {
        return None;
    }
    let mut i = 1usize;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
        } else if bytes[i] == b'"' {
            let raw = &text[1..i];
            let rest = &text[i + 1..];
            let unescaped = raw
                .replace("\\\"", "\"")
                .replace("\\\\", "\\")
                .replace("\\n", "\n")
                .replace("\\r", "\r")
                .replace("\\t", "\t");
            return Some((unescaped, rest));
        } else {
            i += 1;
        }
    }
    None
}

/// Convert a `Term::Const` to its i64, erroring on non-const terms.
fn term_to_const(term: &Term) -> Result<i64, String> {
    match term {
        Term::Const(id) => Ok(*id),
        Term::Var(name) => Err(format!("variable ?{name} not allowed in quoted triple")),
        Term::Wildcard => Err("wildcard not allowed in quoted triple".to_owned()),
        Term::DefaultGraph => Err("default graph not allowed in quoted triple".to_owned()),
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_simple() {
        let text = "?x <p> ?y :- ?x <q> ?z . ?a <b> ?c :- ?d <e> ?f .";
        let rules = tokenize_rules(text);
        assert_eq!(rules.len(), 2);
    }

    #[test]
    fn test_tokenize_with_literal() {
        let text = r#"?x <p> "hello.world" :- ?x <q> ?z ."#;
        let rules = tokenize_rules(text);
        assert_eq!(rules.len(), 1, "dot inside literal should not split");
    }

    #[test]
    fn test_tokenize_comment() {
        let text = "# this is a comment\n?x <p> ?y :- ?x <q> ?z .";
        let rules = tokenize_rules(text);
        assert_eq!(rules.len(), 1);
    }

    #[test]
    fn test_find_neck() {
        let rule = "?x <p> ?y :- ?x <q> ?z";
        let pos = find_neck(rule).unwrap();
        assert_eq!(&rule[pos..pos + 2], ":-");
    }

    #[test]
    fn test_parse_comparison() {
        let lit = "?a > 18";
        let result = try_parse_comparison(lit);
        assert!(result.is_some());
        if let Some(BodyLiteral::Compare(_, op, _)) = result {
            assert_eq!(op, CompareOp::Gt);
        }
    }

    #[test]
    fn test_split_body_simple() {
        let body = "?x <p> ?y, ?y <q> ?z";
        let parts = split_body(body);
        assert_eq!(parts.len(), 2);
    }
}
