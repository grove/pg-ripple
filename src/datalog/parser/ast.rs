use super::lexer::{self, split_body};
use crate::datalog::{
    AggFunc, AggregateLiteral, ArithOp, Atom, BodyLiteral, CompareOp, StringBuiltin,
    TemporalFilter, Term,
};
/// Parse the head of a rule (single atom, optionally with GRAPH clause).
pub(super) fn parse_head(text: &str) -> Result<Atom, String> {
    parse_atom(text.trim())
}

/// Parse the body: a comma-separated list of literals.
pub(super) fn parse_body(text: &str) -> Result<Vec<BodyLiteral>, String> {
    let literals = lexer::split_body(text);
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

    // Temporal operators (v0.106.0): AFTER(...), BEFORE(...), DURING(...)
    if let Some(tf) = try_parse_temporal_filter(text) {
        return Ok(BodyLiteral::TemporalFilter(tf));
    }

    // Aggregate literal: COUNT(?y WHERE ...) = ?n, SUM(...), MIN(...), MAX(...), AVG(...)
    if let Some(agg) = try_parse_aggregate(text) {
        return Ok(BodyLiteral::Aggregate(agg));
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

/// Try parsing a temporal filter body literal (v0.106.0 + v0.107.0).
///
/// Recognized forms:
/// - `AFTER('2025-01-01'::timestamptz)` or `AFTER('2025-01-01'::xsd:dateTime)`
/// - `BEFORE('2025-01-01'::timestamptz)`
/// - `DURING('2025-01-01', '2025-12-31')`
/// - `WITHIN(?s, ex:pred, ?o, 'P3D')` — (v0.107.0) fact held within last duration
/// - `SEQUENCE(?s1, pred1, ?o1, ?s2, pred2, ?o2, 'PT1H')` — (v0.107.0) A before B within window
/// - `CONSECUTIVE(3, ex:pred, 'P3D')` — (v0.107.0) n consecutive readings within window
fn try_parse_temporal_filter(text: &str) -> Option<TemporalFilter> {
    let upper = text.to_uppercase();
    let trim_cast = |s: &str| -> String {
        // Remove cast suffixes like `::timestamptz`, `::xsd:dateTime`.
        if let Some(pos) = s.rfind("::") {
            s[..pos].trim().to_owned()
        } else {
            s.trim().to_owned()
        }
    };

    if upper.starts_with("AFTER(") && text.ends_with(')') {
        let inner = &text[6..text.len() - 1];
        let ts = trim_cast(inner);
        return Some(TemporalFilter::After(ts));
    }

    if upper.starts_with("BEFORE(") && text.ends_with(')') {
        let inner = &text[7..text.len() - 1];
        let ts = trim_cast(inner);
        return Some(TemporalFilter::Before(ts));
    }

    if upper.starts_with("DURING(") && text.ends_with(')') {
        let inner = &text[7..text.len() - 1];
        let parts = lexer::split_csv(inner);
        if parts.len() == 2 {
            let from_ts = trim_cast(&parts[0]);
            let to_ts = trim_cast(&parts[1]);
            return Some(TemporalFilter::During(from_ts, to_ts));
        }
    }

    // v0.107.0: WITHIN(?s, predicate, ?o, duration)
    // The duration is the last argument; it can be an ISO 8601 interval string.
    if upper.starts_with("WITHIN(") && text.ends_with(')') {
        let inner = &text[7..text.len() - 1];
        let parts = lexer::split_csv(inner);
        if parts.len() == 4 {
            // The duration is the last component.
            let duration = trim_cast(&parts[3]);
            return Some(TemporalFilter::Within(duration));
        }
    }

    // v0.107.0: SEQUENCE(s1_var, pred1, o1_var, s2_var, pred2, o2_var, window)
    if upper.starts_with("SEQUENCE(") && text.ends_with(')') {
        let inner = &text[9..text.len() - 1];
        let parts = lexer::split_csv(inner);
        if parts.len() == 7 {
            let s1 = parts[0].trim().to_owned();
            let p1 = parts[1].trim().to_owned();
            let o1 = parts[2].trim().to_owned();
            let s2 = parts[3].trim().to_owned();
            let p2 = parts[4].trim().to_owned();
            let o2 = parts[5].trim().to_owned();
            let window = trim_cast(&parts[6]);
            return Some(TemporalFilter::Sequence(s1, p1, o1, s2, p2, o2, window));
        }
    }

    // v0.107.0: CONSECUTIVE(n, predicate, window)
    if upper.starts_with("CONSECUTIVE(") && text.ends_with(')') {
        let inner = &text[12..text.len() - 1];
        let parts = lexer::split_csv(inner);
        if parts.len() == 3 {
            let n_str = parts[0].trim();
            let pred = parts[1].trim().to_owned();
            let window = trim_cast(&parts[2]);
            if let Ok(n) = n_str.parse::<i64>() {
                return Some(TemporalFilter::Consecutive(n, pred, window));
            }
        }
    }

    // v0.118.0: Allen's interval relation operators
    // Each takes four timestamp arguments: (a_start, a_end, b_start, b_end)
    for (prefix, len) in &[
        ("ALLEN_BEFORE(", 13),
        ("ALLEN_MEETS(", 12),
        ("ALLEN_OVERLAPS(", 15),
        ("ALLEN_DURING(", 13),
        ("ALLEN_FINISHES(", 15),
        ("ALLEN_STARTS(", 13),
        ("ALLEN_EQUALS(", 13),
    ] {
        if upper.starts_with(prefix) && text.ends_with(')') {
            let inner = &text[*len..text.len() - 1];
            let parts = lexer::split_csv(inner);
            if parts.len() == 4 {
                let a_start = trim_cast(&parts[0]);
                let a_end = trim_cast(&parts[1]);
                let b_start = trim_cast(&parts[2]);
                let b_end = trim_cast(&parts[3]);
                let variant = match upper.split('(').next().unwrap_or("") {
                    "ALLEN_BEFORE" => TemporalFilter::AllenBefore(a_start, a_end, b_start, b_end),
                    "ALLEN_MEETS" => TemporalFilter::AllenMeets(a_start, a_end, b_start, b_end),
                    "ALLEN_OVERLAPS" => {
                        TemporalFilter::AllenOverlaps(a_start, a_end, b_start, b_end)
                    }
                    "ALLEN_DURING" => TemporalFilter::AllenDuring(a_start, a_end, b_start, b_end),
                    "ALLEN_FINISHES" => {
                        TemporalFilter::AllenFinishes(a_start, a_end, b_start, b_end)
                    }
                    "ALLEN_STARTS" => TemporalFilter::AllenStarts(a_start, a_end, b_start, b_end),
                    "ALLEN_EQUALS" => TemporalFilter::AllenEquals(a_start, a_end, b_start, b_end),
                    _ => continue,
                };
                return Some(variant);
            }
        }
    }

    None
}

/// Split a CSV string respecting quoted strings and nested parentheses.
/// Returns a `Vec<String>` of the comma-separated parts (un-trimmed).
fn try_parse_aggregate(text: &str) -> Option<AggregateLiteral> {
    let upper = text.to_uppercase();
    let func = if upper.starts_with("COUNT(") {
        AggFunc::Count
    } else if upper.starts_with("SUM(") {
        AggFunc::Sum
    } else if upper.starts_with("MIN(") {
        AggFunc::Min
    } else if upper.starts_with("MAX(") {
        AggFunc::Max
    } else if upper.starts_with("AVG(") {
        AggFunc::Avg
    } else {
        return None;
    };

    // Find opening paren position.
    let paren_start = text.find('(')?;

    // Find " WHERE " keyword (case-insensitive) inside the outer parens.
    let after_paren = &text[paren_start + 1..];
    let where_pos_in_after = after_paren.to_uppercase().find(" WHERE ")?;

    // Extract agg_var: between '(' and ' WHERE '.
    let agg_var_str = after_paren[..where_pos_in_after].trim();
    let agg_var = agg_var_str.strip_prefix('?')?.to_owned();

    // Find the body atom: between WHERE and ')'.
    let after_where = &after_paren[where_pos_in_after + 7..]; // skip " WHERE "

    // Find the closing paren for the aggregate function.
    // We need to find the ')' that closes the aggregate, respecting nesting.
    let close_paren = {
        let mut depth = 1usize;
        let mut pos = None;
        for (i, c) in after_where.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        pos = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        pos?
    };

    let atom_str = after_where[..close_paren].trim();
    let atom = parse_atom(atom_str).ok()?;

    // After ')' we expect `= ?resultVar` or `) = ?resultVar`.
    let after_close = after_where[close_paren + 1..].trim();
    let result_str = after_close
        .strip_prefix('=')
        .map(str::trim)
        .and_then(|s| s.strip_prefix('?'))?
        .trim()
        .to_owned();

    Some(AggregateLiteral {
        func,
        agg_var,
        atom,
        result_var: result_str,
    })
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
pub(super) fn try_parse_comparison(text: &str) -> Option<BodyLiteral> {
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
    // ── v0.109.0 NS-RL string similarity built-ins ────────────────────────────
    if text.starts_with("pg:trigram_similarity(") {
        return try_parse_similarity_two_arg(text, "pg:trigram_similarity(", |a, b, op, rhs| {
            StringBuiltin::TrigramSimilarity(a, b, op, rhs)
        });
    }
    if text.starts_with("pg:levenshtein(") {
        return try_parse_similarity_two_arg(text, "pg:levenshtein(", |a, b, op, rhs| {
            StringBuiltin::Levenshtein(a, b, op, rhs)
        });
    }
    if text.starts_with("pg:soundex(") {
        // pg:soundex(?s) OP ?code  — one string argument, one comparison term
        return try_parse_soundex(text);
    }
    if text.starts_with("pg:metaphone(") {
        return try_parse_metaphone(text);
    }
    if text.starts_with("pg:jaro_winkler(") {
        return try_parse_similarity_two_arg(text, "pg:jaro_winkler(", |a, b, op, rhs| {
            StringBuiltin::JaroWinkler(a, b, op, rhs)
        });
    }
    // ── v0.111.0 PPRL Bloom-filter ────────────────────────────────────────────
    if text.starts_with("pg:dice_similarity(") {
        return try_parse_similarity_two_arg(text, "pg:dice_similarity(", |a, b, op, rhs| {
            StringBuiltin::DiceSimilarity(a, b, op, rhs)
        });
    }
    None
}

/// Parse `pg:X(?a, ?b) OP ?rhs` where X takes two term arguments.
fn try_parse_similarity_two_arg<F>(text: &str, prefix: &str, build: F) -> Option<BodyLiteral>
where
    F: Fn(Term, Term, CompareOp, Term) -> StringBuiltin,
{
    let inner_start = prefix.len();
    let inner_end = text[inner_start..].find(')')? + inner_start;
    let inner = &text[inner_start..inner_end];
    let parts = split_body(inner);
    if parts.len() < 2 {
        return None;
    }
    let a = parse_term_simple(parts[0].trim()).ok()?;
    let b = parse_term_simple(parts[1].trim()).ok()?;
    let rest = text[inner_end + 1..].trim();
    let (op, rhs_str) = parse_compare_op_rest(rest)?;
    let rhs = parse_term_simple(rhs_str).ok()?;
    Some(BodyLiteral::StringBuiltin(build(a, b, op, rhs)))
}

/// Parse `pg:soundex(?s) OP ?rhs`.
fn try_parse_soundex(text: &str) -> Option<BodyLiteral> {
    let inner_start = "pg:soundex(".len();
    let inner_end = text[inner_start..].find(')')? + inner_start;
    let inner = &text[inner_start..inner_end];
    let s = parse_term_simple(inner.trim()).ok()?;
    let rest = text[inner_end + 1..].trim();
    let (op, rhs_str) = parse_compare_op_rest(rest)?;
    let rhs = parse_term_simple(rhs_str).ok()?;
    Some(BodyLiteral::StringBuiltin(StringBuiltin::Soundex(
        s, op, rhs,
    )))
}

/// Parse `pg:metaphone(?s, 4) OP ?rhs`.
fn try_parse_metaphone(text: &str) -> Option<BodyLiteral> {
    let inner_start = "pg:metaphone(".len();
    let inner_end = text[inner_start..].find(')')? + inner_start;
    let inner = &text[inner_start..inner_end];
    let parts = split_body(inner);
    if parts.len() < 2 {
        return None;
    }
    let s = parse_term_simple(parts[0].trim()).ok()?;
    let maxlen: i64 = parts[1].trim().trim_matches('"').parse().ok()?;
    let rest = text[inner_end + 1..].trim();
    let (op, rhs_str) = parse_compare_op_rest(rest)?;
    let rhs = parse_term_simple(rhs_str).ok()?;
    Some(BodyLiteral::StringBuiltin(StringBuiltin::Metaphone(
        s, maxlen, op, rhs,
    )))
}

/// Parse a comparison operator and the remainder RHS string.
fn parse_compare_op_rest(rest: &str) -> Option<(CompareOp, &str)> {
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
    Some((op, parts[1].trim()))
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
    let tokens = lexer::tokenize_terms(text);
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
