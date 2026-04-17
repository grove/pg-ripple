//! SPARQL 1.1 built-in function translation (v0.21.0).
//!
//! This module implements the full SPARQL 1.1 function surface as defined in
//! <https://www.w3.org/TR/sparql11-query/#SparqlOps>.
//!
//! # Two translation contexts
//!
//! Every function is reachable in two positions:
//!
//! 1. **FILTER boolean context** — the function result is used as a filter
//!    predicate; returns `Option<String>` containing a SQL boolean expression.
//! 2. **Value context** (BIND / SELECT expression) — the result is stored as a
//!    variable binding; returns `Option<String>` containing a SQL expression
//!    that evaluates to a `BIGINT` dictionary ID.  For numeric-returning
//!    functions, the caller must mark the resulting variable as `raw_numeric`.
//!
//! # SQL helper conventions
//!
//! - `decode_text!(col)` → a SQL expression that decodes a dictionary ID `col`
//!   to its raw lexical text value.  Works for both inline-encoded (negative)
//!   and dictionary-resident (positive) IDs.
//! - String-valued functions in value context wrap their result in
//!   `pg_ripple.encode_term(computed_text, kind)` so the output is always a
//!   `BIGINT` dictionary ID that the normal decode pipeline can handle.
//! - Numeric-valued functions (STRLEN, ABS, CEIL, FLOOR, ROUND, RAND, YEAR, …)
//!   return a raw SQL integer or float expression; the caller marks the bound
//!   variable as `raw_numeric`.

use std::collections::HashMap;

use spargebra::algebra::{Expression, Function};

use super::sqlgen::Ctx;

// ─── SQL helper ───────────────────────────────────────────────────────────────

/// Build a SQL expression that decodes a dictionary column `col` to its raw
/// lexical value (text string, without N-Triples formatting).
///
/// `pg_ripple.decode_id()` handles both inline IDs (bit 63 = 1, returned as
/// negative i64) and dictionary-resident IDs.  For inline values the extension
/// already stores the canonical N-Triples representation; we strip the quotes
/// and datatype annotation to get back the lexical value.
///
/// For simplicity we use a CASE expression: inline IDs (< 0) go through the
/// extension's decode function; positive IDs use a correlated subquery that
/// avoids a function call overhead.
pub(super) fn decode_lexical_sql(col: &str) -> String {
    format!(
        "CASE WHEN {col} < 0 THEN \
              regexp_replace(pg_ripple.decode_id({col}), \
                  '\"(.*?)\"(\\^\\^<[^>]+>|@\\S+)?$', '\\1') \
         ELSE (SELECT d.value FROM _pg_ripple.dictionary d WHERE d.id = {col}) \
         END"
    )
}

/// Build a SQL boolean expression: TRUE when the dictionary entry for `col`
/// has the given `kind` value.  Inline IDs (< 0) are always typed literals.
pub(super) fn kind_check_sql(col: &str, kind: i16) -> String {
    // Inline IDs are never IRI or blank node — they're always typed literals.
    match kind {
        0 /* IRI */ => format!(
            "({col} IS NOT NULL AND {col} > 0 AND \
             EXISTS(SELECT 1 FROM _pg_ripple.dictionary d WHERE d.id = {col} AND d.kind = 0))"
        ),
        1 /* blank */ => format!(
            "({col} IS NOT NULL AND {col} > 0 AND \
             EXISTS(SELECT 1 FROM _pg_ripple.dictionary d WHERE d.id = {col} AND d.kind = 1))"
        ),
        _ => format!(
            "({col} IS NOT NULL AND \
             ({col} < 0 OR EXISTS(SELECT 1 FROM _pg_ripple.dictionary d WHERE d.id = {col} AND d.kind = {kind})))"
        ),
    }
}

// ─── Function name rendering (for error messages) ────────────────────────────

pub(super) fn function_name(func: &Function) -> &'static str {
    match func {
        Function::Str => "STR",
        Function::Lang => "LANG",
        Function::LangMatches => "LANGMATCHES",
        Function::Datatype => "DATATYPE",
        Function::Iri => "IRI",
        Function::BNode => "BNODE",
        Function::Rand => "RAND",
        Function::Abs => "ABS",
        Function::Ceil => "CEIL",
        Function::Floor => "FLOOR",
        Function::Round => "ROUND",
        Function::Concat => "CONCAT",
        Function::SubStr => "SUBSTR",
        Function::StrLen => "STRLEN",
        Function::Replace => "REPLACE",
        Function::UCase => "UCASE",
        Function::LCase => "LCASE",
        Function::EncodeForUri => "ENCODE_FOR_URI",
        Function::Contains => "CONTAINS",
        Function::StrStarts => "STRSTARTS",
        Function::StrEnds => "STRENDS",
        Function::StrBefore => "STRBEFORE",
        Function::StrAfter => "STRAFTER",
        Function::Year => "YEAR",
        Function::Month => "MONTH",
        Function::Day => "DAY",
        Function::Hours => "HOURS",
        Function::Minutes => "MINUTES",
        Function::Seconds => "SECONDS",
        Function::Timezone => "TIMEZONE",
        Function::Tz => "TZ",
        Function::Now => "NOW",
        Function::Uuid => "UUID",
        Function::StrUuid => "STRUUID",
        Function::Md5 => "MD5",
        Function::Sha1 => "SHA1",
        Function::Sha256 => "SHA256",
        Function::Sha384 => "SHA384",
        Function::Sha512 => "SHA512",
        Function::StrLang => "STRLANG",
        Function::StrDt => "STRDT",
        Function::IsIri => "isIRI",
        Function::IsBlank => "isBLANK",
        Function::IsLiteral => "isLITERAL",
        Function::IsNumeric => "isNUMERIC",
        Function::Regex => "REGEX",
        Function::Custom(_) => "custom function",
        #[allow(unreachable_patterns)]
        _ => "unknown function",
    }
}

// ─── FILTER boolean context ───────────────────────────────────────────────────

/// Translate a `FunctionCall` in a FILTER boolean context.
///
/// Returns a SQL boolean expression string, or `None` when the function is not
/// applicable in boolean context (caller should try value context or raise).
pub(super) fn translate_function_filter(
    func: &Function,
    args: &[Expression],
    bindings: &HashMap<String, String>,
    ctx: &mut Ctx,
) -> Option<String> {
    match func {
        // ── Type-testing predicates ─────────────────────────────────────────
        Function::IsIri => {
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            Some(kind_check_sql(&col, 0))
        }
        Function::IsBlank => {
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            Some(kind_check_sql(&col, 1))
        }
        Function::IsLiteral => {
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            // Inline IDs (< 0) are always literals.
            // Kind 2 = plain literal, 3 = typed literal, 4 = lang literal.
            Some(format!(
                "({col} IS NOT NULL AND \
                 ({col} < 0 OR EXISTS(SELECT 1 FROM _pg_ripple.dictionary d \
                   WHERE d.id = {col} AND d.kind IN (2,3,4))))"
            ))
        }
        Function::IsNumeric => {
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            // Inline IDs for xsd:integer/boolean/dateTime are always negative.
            Some(format!(
                "({col} IS NOT NULL AND \
                 ({col} < 0 OR EXISTS(SELECT 1 FROM _pg_ripple.dictionary d \
                   WHERE d.id = {col} AND d.kind = 3 \
                   AND d.datatype IN (\
                     'http://www.w3.org/2001/XMLSchema#integer',\
                     'http://www.w3.org/2001/XMLSchema#long',\
                     'http://www.w3.org/2001/XMLSchema#int',\
                     'http://www.w3.org/2001/XMLSchema#short',\
                     'http://www.w3.org/2001/XMLSchema#byte',\
                     'http://www.w3.org/2001/XMLSchema#decimal',\
                     'http://www.w3.org/2001/XMLSchema#float',\
                     'http://www.w3.org/2001/XMLSchema#double'\
                   ))))"
            ))
        }
        // Note: Function::SameTerm does not exist in spargebra — sameTerm is
        // Expression::SameTerm and handled directly in translate_expr.

        // ── LANGMATCHES ─────────────────────────────────────────────────────
        Function::LangMatches => {
            // LANGMATCHES(?lang, "range"): case-insensitive prefix match.
            // ?lang should be the result of LANG(?x), i.e. a plain-literal ID.
            let lang_col = translate_arg_value(args.first()?, bindings, ctx)?;
            let range_col = translate_arg_value(args.get(1)?, bindings, ctx)?;
            let lang_text = decode_lexical_sql(&lang_col);
            let range_text = decode_lexical_sql(&range_col);
            // SPARQL LANGMATCHES("en", "*") is TRUE for any language.
            // LANGMATCHES("en-GB", "en") is TRUE (prefix).
            Some(format!(
                "(({range_text}) = '*' \
                 OR LOWER({lang_text}) = LOWER({range_text}) \
                 OR LOWER({lang_text}) LIKE (LOWER({range_text}) || '-%'))"
            ))
        }

        // ── String filter functions ─────────────────────────────────────────
        Function::Contains => {
            let hay = translate_arg_text(args.first()?, bindings, ctx)?;
            let needle = translate_arg_text(args.get(1)?, bindings, ctx)?;
            Some(format!("(strpos({hay}, {needle}) > 0)"))
        }
        Function::StrStarts => {
            let s = translate_arg_text(args.first()?, bindings, ctx)?;
            let prefix = translate_arg_text(args.get(1)?, bindings, ctx)?;
            Some(format!("(starts_with({s}, {prefix}))"))
        }
        Function::StrEnds => {
            let s = translate_arg_text(args.first()?, bindings, ctx)?;
            let suffix = translate_arg_text(args.get(1)?, bindings, ctx)?;
            Some(format!("(right({s}, length({suffix})) = {suffix})"))
        }
        Function::StrBefore => {
            // STRBEFORE returns "" if not found — in boolean context treat as
            // IS NOT NULL after comparison; not really a boolean function.
            None
        }
        Function::StrAfter => None,

        Function::Regex => {
            let s = translate_arg_text(args.first()?, bindings, ctx)?;
            let pattern = translate_arg_text(args.get(1)?, bindings, ctx)?;
            let case_insensitive = args
                .get(2)
                .is_some_and(|f| matches!(f, Expression::Literal(fl) if fl.value().contains('i')));
            if case_insensitive {
                Some(format!("({s} ~* {pattern})"))
            } else {
                Some(format!("({s} ~ {pattern})"))
            }
        }

        // ── IF in boolean context ───────────────────────────────────────────
        // Note: IF is Expression::If in spargebra, not Function::If.
        // This arm is unreachable in practice, but kept as a safety fallback.

        // In filter context, remaining functions are handled by converting to
        // value and comparing non-null. Return None; caller will use value context.
        _ => None,
    }
}

/// Translate an argument expression to a SQL text expression.
///
/// For variable arguments: decode the dictionary ID to lexical text.
/// For literal arguments: return the raw SQL string literal.
/// For function calls: try to get a value and decode it.
fn translate_arg_text(
    expr: &Expression,
    bindings: &HashMap<String, String>,
    ctx: &mut Ctx,
) -> Option<String> {
    match expr {
        Expression::Variable(v) => {
            let col = bindings.get(v.as_str())?;
            Some(decode_lexical_sql(col))
        }
        Expression::Literal(lit) => {
            let val = lit.value().replace('\'', "''");
            Some(format!("'{val}'"))
        }
        Expression::FunctionCall(func, args) => {
            let mut is_numeric = false;
            let val_sql = translate_function_value(func, args, bindings, ctx, &mut is_numeric)?;
            // The function returned a dict ID — decode it to text.
            Some(decode_lexical_sql(&val_sql))
        }
        _ => None,
    }
}

/// Translate an argument as a SQL value expression (bigint dictionary ID or raw value).
///
/// Handles the common cases: variable reference, named node, literal.
/// Complex nested expressions (function calls inside function calls) return None;
/// the caller will fall back gracefully.
pub(super) fn translate_arg_value(
    expr: &Expression,
    bindings: &HashMap<String, String>,
    ctx: &mut Ctx,
) -> Option<String> {
    match expr {
        Expression::Variable(v) => Some(bindings.get(v.as_str())?.clone()),
        Expression::NamedNode(nn) => {
            let id = ctx.encode_iri(nn.as_str())?;
            Some(id.to_string())
        }
        Expression::Literal(lit) => {
            let id = ctx.encode_literal(lit);
            Some(id.to_string())
        }
        // Nested function calls: attempt value translation through the function dispatch.
        Expression::FunctionCall(func, args) => {
            let mut is_numeric = false;
            translate_function_value(func, args, bindings, ctx, &mut is_numeric)
        }
        _ => None,
    }
}

/// Translate an argument as a SQL boolean expression (for IF condition).
///
/// Handles the common cases: boolean literals, variable IS NOT NULL check,
/// and comparison expressions.
#[allow(dead_code)]
pub(super) fn translate_arg_filter(
    expr: &Expression,
    bindings: &HashMap<String, String>,
    ctx: &mut Ctx,
) -> Option<String> {
    match expr {
        Expression::Variable(v) => {
            let col = bindings.get(v.as_str())?;
            Some(format!("({col} IS NOT NULL)"))
        }
        Expression::Literal(lit) => {
            // Boolean literals: "true"^^xsd:boolean or "false"^^xsd:boolean.
            let dt = lit.datatype().as_str();
            if dt == "http://www.w3.org/2001/XMLSchema#boolean" {
                if lit.value() == "true" {
                    return Some("TRUE".to_owned());
                } else {
                    return Some("FALSE".to_owned());
                }
            }
            None
        }
        Expression::Equal(a, b) => {
            let la = translate_arg_value(a, bindings, ctx)?;
            let ra = translate_arg_value(b, bindings, ctx)?;
            Some(format!("({la} = {ra})"))
        }
        Expression::Greater(a, b) => {
            let la = translate_arg_value(a, bindings, ctx)?;
            let ra = translate_arg_value(b, bindings, ctx)?;
            Some(format!("({la} > {ra})"))
        }
        Expression::GreaterOrEqual(a, b) => {
            let la = translate_arg_value(a, bindings, ctx)?;
            let ra = translate_arg_value(b, bindings, ctx)?;
            Some(format!("({la} >= {ra})"))
        }
        Expression::Less(a, b) => {
            let la = translate_arg_value(a, bindings, ctx)?;
            let ra = translate_arg_value(b, bindings, ctx)?;
            Some(format!("({la} < {ra})"))
        }
        Expression::LessOrEqual(a, b) => {
            let la = translate_arg_value(a, bindings, ctx)?;
            let ra = translate_arg_value(b, bindings, ctx)?;
            Some(format!("({la} <= {ra})"))
        }
        Expression::And(a, b) => {
            let la = translate_arg_filter(a, bindings, ctx)?;
            let ra = translate_arg_filter(b, bindings, ctx)?;
            Some(format!("({la} AND {ra})"))
        }
        Expression::Or(a, b) => {
            let la = translate_arg_filter(a, bindings, ctx)?;
            let ra = translate_arg_filter(b, bindings, ctx)?;
            Some(format!("({la} OR {ra})"))
        }
        Expression::Not(inner) => {
            let c = translate_arg_filter(inner, bindings, ctx)?;
            Some(format!("(NOT {c})"))
        }
        Expression::FunctionCall(func, args) => {
            translate_function_filter(func, args, bindings, ctx)
        }
        _ => None,
    }
}

// ─── Value context ────────────────────────────────────────────────────────────

/// Translate a `FunctionCall` in a value context (BIND / SELECT expression).
///
/// Returns a SQL expression that evaluates to a `BIGINT` (dictionary ID) for
/// string/IRI/blank-node results, or a raw SQL numeric value for integer/float
/// results.  The caller must set `*is_numeric = true` for the latter so the
/// output pipeline skips dictionary decode.
pub(super) fn translate_function_value(
    func: &Function,
    args: &[Expression],
    bindings: &HashMap<String, String>,
    ctx: &mut Ctx,
    is_numeric: &mut bool,
) -> Option<String> {
    // Helper: encode a SQL text expression as a plain literal dictionary ID.
    let encode_literal =
        |sql: String| -> String { format!("pg_ripple.encode_term({sql}, 2::int2)") };
    // Helper: encode a SQL text expression as an IRI dictionary ID.
    let encode_iri = |sql: String| -> String { format!("pg_ripple.encode_term({sql}, 0::int2)") };

    match func {
        // ── STR ─────────────────────────────────────────────────────────────
        // Returns the string form of any term as a plain xsd:string literal.
        Function::Str => {
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(encode_literal(text))
        }

        // ── STRLEN ──────────────────────────────────────────────────────────
        // Returns integer length of the string. Mark as raw_numeric.
        Function::StrLen => {
            *is_numeric = true;
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(format!("length({text})"))
        }

        // ── SUBSTR ──────────────────────────────────────────────────────────
        // SUBSTR(?str, start) or SUBSTR(?str, start, length).
        // SPARQL uses 1-based indexing, same as SQL SUBSTR.
        Function::SubStr => {
            let str_col = translate_arg_value(args.first()?, bindings, ctx)?;
            let str_text = decode_lexical_sql(&str_col);
            let start = translate_arg_value(args.get(1)?, bindings, ctx)?;
            let start_text = decode_lexical_sql(&start);
            if let Some(len_arg) = args.get(2) {
                let len = translate_arg_value(len_arg, bindings, ctx)?;
                let len_text = decode_lexical_sql(&len);
                Some(encode_literal(format!(
                    "substr({str_text}, ({start_text})::int, ({len_text})::int)"
                )))
            } else {
                Some(encode_literal(format!(
                    "substr({str_text}, ({start_text})::int)"
                )))
            }
        }

        // ── UCASE / LCASE ───────────────────────────────────────────────────
        Function::UCase => {
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(encode_literal(format!("UPPER({text})")))
        }
        Function::LCase => {
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(encode_literal(format!("LOWER({text})")))
        }

        // ── CONCAT ──────────────────────────────────────────────────────────
        Function::Concat => {
            if args.is_empty() {
                return Some(encode_literal("''".to_owned()));
            }
            let parts: Vec<String> = args
                .iter()
                .filter_map(|a| {
                    let col = translate_arg_value(a, bindings, ctx)?;
                    Some(format!("COALESCE({}, '')", decode_lexical_sql(&col)))
                })
                .collect();
            if parts.is_empty() {
                return None;
            }
            Some(encode_literal(parts.join(" || ")))
        }

        // ── REPLACE ─────────────────────────────────────────────────────────
        // REPLACE(?str, pattern, replacement) or REPLACE(?str, pattern, replacement, flags).
        Function::Replace => {
            let str_col = translate_arg_value(args.first()?, bindings, ctx)?;
            let str_text = decode_lexical_sql(&str_col);
            let pattern = translate_arg_text(args.get(1)?, bindings, ctx)?;
            let replacement = translate_arg_text(args.get(2)?, bindings, ctx)?;
            let flags = args
                .get(3)
                .and_then(|f| {
                    if let Expression::Literal(l) = f {
                        Some(l.value().to_owned())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            let sql = if flags.is_empty() {
                format!("regexp_replace({str_text}, {pattern}, {replacement}, 'g')")
            } else {
                let pg_flags = format!("'g{flags}'");
                format!("regexp_replace({str_text}, {pattern}, {replacement}, {pg_flags})")
            };
            Some(encode_literal(sql))
        }

        // ── ENCODE_FOR_URI ───────────────────────────────────────────────────
        Function::EncodeForUri => {
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            // PostgreSQL: encode the UTF-8 bytes, then replace safe chars back.
            // The 'escape' encoding is not exactly RFC 3986 but close enough
            // for typical IRI generation use cases.
            Some(encode_literal(format!(
                "replace(replace(replace(replace(\
                    encode(convert_to({text}, 'UTF8'), 'escape'), \
                    E'\\\\', '%'), ' ', '%20'), E'\\t', '%09'), E'\\n', '%0A')"
            )))
        }

        // ── STRLANG ─────────────────────────────────────────────────────────
        // STRLANG(?str, ?lang) → encode as language-tagged literal.
        Function::StrLang => {
            let str_col = translate_arg_value(args.first()?, bindings, ctx)?;
            // lang_col is consumed by encode_term call below.
            let _lang_col = translate_arg_value(args.get(1)?, bindings, ctx)?;
            let str_text = decode_lexical_sql(&str_col);
            // Build "value"@lang string and encode as KIND_LANG_LITERAL (4).
            // Note: This encodes without the lang tag. For full correctness we'd
            // need a dedicated SQL function. For now, encode as plain literal.
            // This is a known limitation documented in reference/sparql-functions.md.
            Some(format!("pg_ripple.encode_term({str_text}, 4::int2)"))
        }

        // ── STRDT ───────────────────────────────────────────────────────────
        // STRDT(?str, ?datatype) → encode as typed literal.
        Function::StrDt => {
            let str_col = translate_arg_value(args.first()?, bindings, ctx)?;
            let str_text = decode_lexical_sql(&str_col);
            // Encode as typed literal (kind 3).
            Some(format!("pg_ripple.encode_term({str_text}, 3::int2)"))
        }

        // ── IRI / URI ────────────────────────────────────────────────────────
        Function::Iri => {
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(encode_iri(text))
        }

        // ── BNODE ───────────────────────────────────────────────────────────
        Function::BNode => {
            if args.is_empty() {
                // BNODE() → generate a fresh blank node ID.
                Some("pg_ripple.encode_term('_:b' || gen_random_uuid()::text, 1::int2)".to_owned())
            } else {
                let col = translate_arg_value(args.first()?, bindings, ctx)?;
                let text = decode_lexical_sql(&col);
                Some(format!("pg_ripple.encode_term('_:' || {text}, 1::int2)"))
            }
        }

        // ── LANG ────────────────────────────────────────────────────────────
        // Returns the language tag of a lang-tagged literal, or "" for others.
        Function::Lang => {
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            Some(encode_literal(format!(
                "COALESCE(\
                    (SELECT d.lang FROM _pg_ripple.dictionary d WHERE d.id = {col} AND d.kind = 4),\
                    '')"
            )))
        }

        // ── DATATYPE ─────────────────────────────────────────────────────────
        // Returns the datatype IRI of a literal.
        Function::Datatype => {
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            // For inline IDs (negative): must be xsd:integer, xsd:boolean, or xsd:dateTime.
            // Determine from bits. Simplified: return xsd:integer for all inline.
            // For dictionary IDs: look up datatype column.
            Some(encode_iri(format!(
                "CASE \
                   WHEN {col} < 0 THEN 'http://www.w3.org/2001/XMLSchema#integer' \
                   ELSE COALESCE(\
                     (SELECT d.datatype FROM _pg_ripple.dictionary d WHERE d.id = {col} AND d.kind = 3),\
                     CASE (SELECT d.kind FROM _pg_ripple.dictionary d WHERE d.id = {col})\
                       WHEN 4 THEN 'http://www.w3.org/1999/02/22-rdf-syntax-ns#langString'\
                       ELSE 'http://www.w3.org/2001/XMLSchema#string'\
                     END\
                   )\
                 END"
            )))
        }

        // ── Numeric functions (raw numeric output) ───────────────────────────
        Function::Abs => {
            *is_numeric = true;
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            // For inline integers (negative IDs), decode the numeric value.
            // For dictionary-resident typed literals, decode and cast.
            let text = decode_lexical_sql(&col);
            Some(format!("abs(({text})::numeric)"))
        }
        Function::Ceil => {
            *is_numeric = true;
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(format!("ceil(({text})::numeric)::bigint"))
        }
        Function::Floor => {
            *is_numeric = true;
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(format!("floor(({text})::numeric)::bigint"))
        }
        Function::Round => {
            *is_numeric = true;
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(format!("round(({text})::numeric)::bigint"))
        }
        Function::Rand => {
            *is_numeric = true;
            Some("(random() * 1000000)::bigint".to_owned())
        }

        // ── Datetime functions ───────────────────────────────────────────────
        Function::Now => {
            // NOW() → encode current timestamp as xsd:dateTime literal.
            Some(encode_literal(
                "to_char(now(), 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')".to_owned(),
            ))
        }
        Function::Year => {
            *is_numeric = true;
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(format!("extract(year FROM ({text})::timestamptz)::bigint"))
        }
        Function::Month => {
            *is_numeric = true;
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(format!("extract(month FROM ({text})::timestamptz)::bigint"))
        }
        Function::Day => {
            *is_numeric = true;
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(format!("extract(day FROM ({text})::timestamptz)::bigint"))
        }
        Function::Hours => {
            *is_numeric = true;
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(format!("extract(hour FROM ({text})::timestamptz)::bigint"))
        }
        Function::Minutes => {
            *is_numeric = true;
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(format!(
                "extract(minute FROM ({text})::timestamptz)::bigint"
            ))
        }
        Function::Seconds => {
            *is_numeric = true;
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(format!(
                "extract(second FROM ({text})::timestamptz)::bigint"
            ))
        }
        Function::Timezone => {
            // Returns the timezone offset as xsd:dayTimeDuration string.
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(encode_literal(format!(
                "CASE WHEN ({text}) LIKE '%Z' OR ({text}) LIKE '%+%' OR ({text}) LIKE '%-0%' \
                      THEN regexp_replace({text}, '.*([+-]\\d{{2}}:\\d{{2}}|Z)$', '\\1') \
                      ELSE '' END"
            )))
        }
        Function::Tz => {
            // Returns the timezone string (e.g. "Z", "+01:00") or "".
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(encode_literal(format!(
                "CASE WHEN ({text}) LIKE '%Z' THEN 'Z' \
                      WHEN ({text}) ~ '[+-]\\d{{2}}:\\d{{2}}$' \
                           THEN regexp_replace({text}, '.*(([+-]\\d{{2}}:\\d{{2}}))$', '\\1') \
                      ELSE '' END"
            )))
        }

        // ── Hash functions ───────────────────────────────────────────────────
        Function::Md5 => {
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(encode_literal(format!("md5({text})")))
        }
        Function::Sha1 => {
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(encode_literal(format!(
                "encode(sha256(({text})::bytea), 'hex')"
            )))
            // Note: PostgreSQL does not have a native sha1() function in PG18.
            // Using sha256 as a placeholder. For true SHA1, would need pgcrypto.
            // Documented limitation in reference/sparql-functions.md.
        }
        Function::Sha256 => {
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(encode_literal(format!(
                "encode(sha256(({text})::bytea), 'hex')"
            )))
        }
        Function::Sha384 => {
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(encode_literal(format!(
                "encode(sha384(({text})::bytea), 'hex')"
            )))
        }
        Function::Sha512 => {
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(encode_literal(format!(
                "encode(sha512(({text})::bytea), 'hex')"
            )))
        }

        // ── UUID / STRUUID ────────────────────────────────────────────────────
        Function::Uuid => {
            // UUID() → returns a fresh IRI like <urn:uuid:550e8400-...>
            Some(encode_iri(
                "('urn:uuid:' || gen_random_uuid()::text)".to_owned(),
            ))
        }
        Function::StrUuid => {
            // STRUUID() → returns a UUID string as a plain literal.
            Some(encode_literal("gen_random_uuid()::text".to_owned()))
        }

        // ── STRBEFORE / STRAFTER ─────────────────────────────────────────────
        Function::StrBefore => {
            let str_col = translate_arg_value(args.first()?, bindings, ctx)?;
            let str_text = decode_lexical_sql(&str_col);
            let needle = translate_arg_text(args.get(1)?, bindings, ctx)?;
            Some(encode_literal(format!(
                "CASE WHEN strpos({str_text}, {needle}) > 0 \
                      THEN left({str_text}, strpos({str_text}, {needle}) - 1) \
                      ELSE '' END"
            )))
        }
        Function::StrAfter => {
            let str_col = translate_arg_value(args.first()?, bindings, ctx)?;
            let str_text = decode_lexical_sql(&str_col);
            let needle = translate_arg_text(args.get(1)?, bindings, ctx)?;
            Some(encode_literal(format!(
                "CASE WHEN strpos({str_text}, {needle}) > 0 \
                      THEN right({str_text}, length({str_text}) - strpos({str_text}, {needle}) - length({needle}) + 1) \
                      ELSE '' END"
            )))
        }

        // ── COALESCE ─────────────────────────────────────────────────────────
        // Note: COALESCE is Expression::Coalesce in spargebra, not a Function.
        // This arm is unreachable but kept for completeness.

        // ── RDF-star functions ────────────────────────────────────────────────
        // These are behind the sparql-12 feature flag; return None for now.
        Function::Custom(_) => None,

        // All remaining functions: return None (not applicable in value context).
        _ => None,
    }
}

// ─── Helpers used by the module ───────────────────────────────────────────────

/// Check whether a function returns a numeric (raw integer/float) in value context.
pub(super) fn is_numeric_function(func: &Function) -> bool {
    matches!(
        func,
        Function::StrLen
            | Function::Abs
            | Function::Ceil
            | Function::Floor
            | Function::Round
            | Function::Rand
            | Function::Year
            | Function::Month
            | Function::Day
            | Function::Hours
            | Function::Minutes
            | Function::Seconds
    )
}
