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

// ─── PostGIS availability probe ──────────────────────────────────────────────

/// Build a SQL expression that applies `new_lexical_sql` to the lexical value of
/// `col`, preserving any language tag from the input.
///
/// If `col` is a dictionary-resident lang-tagged literal (kind=4), the result
/// is re-encoded with `pg_ripple.encode_lang_literal(new_lexical_sql, lang)`.
/// Otherwise (plain literal, typed literal, inline) the result is encoded as a
/// plain literal (kind=2) with `pg_ripple.encode_term(new_lexical_sql, 2)`.
pub(super) fn encode_preserving_lang(col: &str, new_lexical_sql: &str) -> String {
    format!(
        "CASE \
          WHEN {col} > 0 AND EXISTS(\
              SELECT 1 FROM _pg_ripple.dictionary d WHERE d.id = {col} AND d.kind = 4) \
          THEN (SELECT pg_ripple.encode_lang_literal({new_lexical_sql}, d.lang) \
                FROM _pg_ripple.dictionary d WHERE d.id = {col}) \
          ELSE pg_ripple.encode_term({new_lexical_sql}, 2::int2) \
        END"
    )
}

/// Returns `true` when PostGIS is installed in the current database.
///
/// Checked by looking for `st_geomfromtext` in `pg_proc`.  The result is
/// evaluated **at SPARQL translation time** so that we can emit plain `false`
/// or `NULL` SQL literals instead of references to PostGIS functions that
/// PostgreSQL would reject at catalog-resolution time.
fn postgis_available() -> bool {
    pgrx::Spi::get_one::<bool>(
        "SELECT EXISTS(SELECT 1 FROM pg_proc WHERE proname = 'st_geomfromtext')",
    )
    .unwrap_or(None)
    .unwrap_or(false)
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
            // raw_iri_vars (UUID results) are always IRIs — shortcut without dict lookup.
            if let Some(Expression::Variable(v)) = args.first() {
                if ctx.is_raw_iri_var(v.as_str()) {
                    let col = bindings.get(v.as_str())?;
                    return Some(format!("({col} IS NOT NULL)"));
                }
            }
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            Some(kind_check_sql(&col, 0))
        }
        Function::IsBlank => {
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            Some(kind_check_sql(&col, 1))
        }
        Function::IsLiteral => {
            // raw_text_vars (STRUUID, GROUP_CONCAT) are always literals — shortcut.
            // raw_iri_vars (UUID) are NOT literals; skip them.
            if let Some(Expression::Variable(v)) = args.first() {
                if ctx.is_raw_text_var(v.as_str()) && !ctx.is_raw_iri_var(v.as_str()) {
                    let col = bindings.get(v.as_str())?;
                    return Some(format!("({col} IS NOT NULL)"));
                }
            }
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
            // Inline IDs: bit 63=1 (negative as i64), bits 62-56 = type code.
            // TYPE_INTEGER=0, TYPE_BOOLEAN=1, TYPE_DATETIME=2, TYPE_DATE=3.
            // Only TYPE_INTEGER is numeric; check bits 62-56 are all zero.
            // Mask 0x7F00000000000000 = 9151314442816847872 selects bits 62-56.
            Some(format!(
                "({col} IS NOT NULL AND \
                 (({col} < 0 AND ({col} & 9151314442816847872::bigint) = 0) OR EXISTS(SELECT 1 FROM _pg_ripple.dictionary d \
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

        // ── GeoSPARQL topological predicates ───────────────────────────────
        // geo:sf* functions are represented as Function::Custom in spargebra.
        // They require PostGIS (ST_GeomFromText). PostGIS availability is
        // probed at translation time so no PostGIS function reference ever
        // appears in the generated SQL when PostGIS is not installed.
        Function::Custom(name) => {
            let iri = name.as_str();
            // Map GeoSPARQL Simple Features topology predicates to PostGIS.
            let postgis_fn = match iri {
                "http://www.opengis.net/def/function/geosparql/sfIntersects" => {
                    Some("ST_Intersects")
                }
                "http://www.opengis.net/def/function/geosparql/sfContains" => Some("ST_Contains"),
                "http://www.opengis.net/def/function/geosparql/sfWithin" => Some("ST_Within"),
                "http://www.opengis.net/def/function/geosparql/sfOverlaps" => Some("ST_Overlaps"),
                "http://www.opengis.net/def/function/geosparql/sfTouches" => Some("ST_Touches"),
                "http://www.opengis.net/def/function/geosparql/sfCrosses" => Some("ST_Crosses"),
                "http://www.opengis.net/def/function/geosparql/sfDisjoint" => Some("ST_Disjoint"),
                "http://www.opengis.net/def/function/geosparql/sfEquals" => Some("ST_Equals"),
                "http://www.opengis.net/def/function/geosparql/ehIntersects" => {
                    Some("ST_Intersects")
                }
                "http://www.opengis.net/def/function/geosparql/ehContains" => Some("ST_Contains"),
                "http://www.opengis.net/def/function/geosparql/ehCoveredBy" => Some("ST_CoveredBy"),
                "http://www.opengis.net/def/function/geosparql/ehCovers" => Some("ST_Covers"),
                _ => None,
            };
            if let Some(pg_fn) = postgis_fn {
                // When PostGIS is absent, emit literal false — no PostGIS
                // function references are included so PostgreSQL catalog
                // resolution succeeds even without the PostGIS extension.
                if !postgis_available() {
                    return Some("false".to_string());
                }
                let a_col = translate_arg_value(args.first()?, bindings, ctx)?;
                let b_col = translate_arg_value(args.get(1)?, bindings, ctx)?;
                let a_wkt = decode_lexical_sql(&a_col);
                let b_wkt = decode_lexical_sql(&b_col);
                Some(format!(
                    "{pg_fn}(\
                        ST_GeomFromText({a_wkt}), \
                        ST_GeomFromText({b_wkt})\
                      )"
                ))
            } else {
                None
            }
        }

        // In filter context, remaining functions are handled by converting to
        // value and comparing non-null. Return None; caller will use value context.
        _ => None,
    }
}

// ─── pg:similar() — pgvector cosine similarity (v0.27.0) ─────────────────────

/// `pg:similar` IRI constant.
pub(super) const PG_SIMILAR_IRI: &str = "http://pg-ripple.org/functions/similar";

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
            // raw_text_vars (STRUUID, GROUP_CONCAT) and raw_iri_vars (UUID) hold plain
            // text/IRI strings — return directly without dict-decode roundtrip.
            if ctx.is_raw_text_var(v.as_str()) || ctx.is_raw_iri_var(v.as_str()) {
                return Some(col.clone());
            }
            Some(decode_lexical_sql(col))
        }
        Expression::Literal(lit) => {
            let val = lit.value().replace('\'', "''");
            Some(format!("'{val}'"))
        }
        // STR(?x) shortcut: avoid encode_term → decode roundtrip.
        // STR(?x) in text context = lexical form of ?x = decode_lexical_sql(x_col).
        // This also avoids the PostgreSQL snapshot isolation issue where encode_term
        // inserts a new dict row that the subsequent SELECT can't see in the same stmt.
        // Special case: STR(raw_iri_var) → the IRI text itself (no dict lookup needed).
        Expression::FunctionCall(Function::Str, str_args) => {
            if let Some(Expression::Variable(v)) = str_args.first() {
                if ctx.is_raw_iri_var(v.as_str()) {
                    let col = bindings.get(v.as_str())?;
                    return Some(col.clone());
                }
            }
            let inner_col = translate_arg_value(str_args.first()?, bindings, ctx)?;
            Some(decode_lexical_sql(&inner_col))
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
            // Optimization: STRLEN(raw_text_var) → length directly (no dict lookup).
            if let Some(Expression::Variable(v)) = args.first() {
                if ctx.is_raw_text_var(v.as_str()) {
                    let col = bindings.get(v.as_str())?;
                    return Some(format!("length({col})"));
                }
            }
            // Optimization: STRLEN(STR(raw_iri_var)) → length of the IRI text directly.
            if let Some(Expression::FunctionCall(Function::Str, str_inner)) = args.first() {
                if let Some(Expression::Variable(v)) = str_inner.first() {
                    if ctx.is_raw_iri_var(v.as_str()) {
                        let col = bindings.get(v.as_str())?;
                        return Some(format!("length({col})"));
                    }
                }
            }
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(format!("length({text})"))
        }

        // ── SUBSTR ──────────────────────────────────────────────────────────
        // SUBSTR(?str, start) or SUBSTR(?str, start, length).
        // SPARQL uses 1-based indexing, same as SQL SUBSTR.
        // Preserve the language tag of the input literal.
        Function::SubStr => {
            let str_col = translate_arg_value(args.first()?, bindings, ctx)?;
            let str_text = decode_lexical_sql(&str_col);
            let start = translate_arg_value(args.get(1)?, bindings, ctx)?;
            let start_text = decode_lexical_sql(&start);
            let new_lex = if let Some(len_arg) = args.get(2) {
                let len = translate_arg_value(len_arg, bindings, ctx)?;
                let len_text = decode_lexical_sql(&len);
                format!("substr({str_text}, ({start_text})::int, ({len_text})::int)")
            } else {
                format!("substr({str_text}, ({start_text})::int)")
            };
            Some(encode_preserving_lang(&str_col, &new_lex))
        }

        // ── UCASE / LCASE ───────────────────────────────────────────────────
        // Preserve the language tag of the input literal.
        Function::UCase => {
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(encode_preserving_lang(&col, &format!("UPPER({text})")))
        }
        Function::LCase => {
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(encode_preserving_lang(&col, &format!("LOWER({text})")))
        }

        // ── CONCAT ──────────────────────────────────────────────────────────
        // If all arguments have the same language tag, the result has that tag.
        // Otherwise the result is a plain literal.
        // SPARQL 1.1: arguments must be plain literals, xsd:string, or lang-tagged.
        // Non-string typed literals (integers, doubles, etc.) cause a type error.
        Function::Concat => {
            if args.is_empty() {
                return Some(encode_literal("''".to_owned()));
            }
            let cols: Vec<String> = args
                .iter()
                .filter_map(|a| translate_arg_value(a, bindings, ctx))
                .collect();

            // Build a type guard: each column must be a string-compatible type.
            // Returns NULL for inline integers (< 0) or non-string dict entries.
            fn string_guard_sql(col: &str) -> String {
                format!(
                    "CASE WHEN ({col}) IS NULL THEN NULL \
                     WHEN ({col}) < 0 THEN NULL \
                     WHEN EXISTS(SELECT 1 FROM _pg_ripple.dictionary d WHERE d.id = ({col}) AND d.kind IN (2, 4)) THEN ({col}) \
                     WHEN EXISTS(SELECT 1 FROM _pg_ripple.dictionary d WHERE d.id = ({col}) AND d.kind = 3 AND d.datatype = 'http://www.w3.org/2001/XMLSchema#string') THEN ({col}) \
                     ELSE NULL END"
                )
            }

            // Apply type guard to each arg.
            let guarded_cols: Vec<String> = cols.iter().map(|c| string_guard_sql(c)).collect();

            // All must be non-NULL for CONCAT to succeed.
            let all_valid = guarded_cols
                .iter()
                .map(|g| format!("({g}) IS NOT NULL"))
                .collect::<Vec<_>>()
                .join(" AND ");

            let parts: Vec<String> = cols.iter().map(|col| decode_lexical_sql(col)).collect();
            if parts.is_empty() {
                return None;
            }
            let concat_expr = parts.join(" || ");

            // Determine lang preservation: all dict lang-tagged with same lang.
            if cols.len() == 1 {
                let g = string_guard_sql(&cols[0]);
                Some(format!(
                    "CASE WHEN ({g}) IS NULL THEN NULL ELSE {} END",
                    encode_preserving_lang(&cols[0], &concat_expr)
                ))
            } else {
                // Multi-arg: check all args have same lang via SQL
                let first_col = &cols[0];
                let same_lang_check = cols[1..]
                    .iter()
                    .map(|c| format!(
                        "EXISTS(SELECT 1 FROM _pg_ripple.dictionary a \
                                JOIN _pg_ripple.dictionary b ON a.lang = b.lang \
                                WHERE a.id = {first_col} AND a.kind = 4 AND b.id = {c} AND b.kind = 4)"
                    ))
                    .collect::<Vec<_>>()
                    .join(" AND ");
                Some(format!(
                    "CASE WHEN NOT ({all_valid}) THEN NULL \
                       WHEN {first_col} > 0 \
                         AND EXISTS(SELECT 1 FROM _pg_ripple.dictionary d WHERE d.id = {first_col} AND d.kind = 4) \
                         AND {same_lang_check} \
                       THEN (SELECT pg_ripple.encode_lang_literal({concat_expr}, d.lang) \
                             FROM _pg_ripple.dictionary d WHERE d.id = {first_col}) \
                       ELSE pg_ripple.encode_term({concat_expr}, 2::int2) \
                     END"
                ))
            }
        }

        // ── REPLACE ─────────────────────────────────────────────────────────
        // REPLACE(?str, pattern, replacement) or REPLACE(?str, pattern, replacement, flags).
        // Preserve the language tag of the input literal.
        Function::Replace => {
            let str_col = translate_arg_value(args.first()?, bindings, ctx)?;
            let str_text = decode_lexical_sql(&str_col);
            let pattern = translate_arg_text(args.get(1)?, bindings, ctx)?;
            // Convert SPARQL $N backreferences (XQuery semantics) to PostgreSQL \N.
            // $0 → \& (full match), $1-$9 → \1-\9.
            let replacement = {
                let repl_arg = args.get(2)?;
                if let Expression::Literal(lit) = repl_arg {
                    let raw = lit.value();
                    // Replace $0 with \& then $1-$9 with \1-\9
                    let pg_raw = raw.replace("$0", "\\&");
                    let pg_raw = (1..=9usize).fold(pg_raw, |s, n| {
                        s.replace(&format!("${n}"), &format!("\\{n}"))
                    });
                    format!("'{}'", pg_raw.replace('\'', "''"))
                } else {
                    translate_arg_text(repl_arg, bindings, ctx)?
                }
            };
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
            let new_lex = if flags.is_empty() {
                format!("regexp_replace({str_text}, {pattern}, {replacement}, 'g')")
            } else {
                let pg_flags = format!("'g{flags}'");
                format!("regexp_replace({str_text}, {pattern}, {replacement}, {pg_flags})")
            };
            // Type check: REPLACE is a type error for non-string literals (inline → NULL).
            let result = encode_preserving_lang(&str_col, &new_lex);
            Some(format!(
                "CASE WHEN {str_col} < 0 THEN NULL ELSE {result} END"
            ))
        }

        // ── ENCODE_FOR_URI ───────────────────────────────────────────────────
        // RFC 3986 percent-encoding: unreserved chars (A-Za-z0-9-_.~) pass through;
        // all others are encoded as %XX per UTF-8 byte.
        Function::EncodeForUri => {
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(encode_literal(format!(
                "(SELECT string_agg(\
                    CASE WHEN chr ~ '^[A-Za-z0-9\\-_.~]$' THEN chr \
                         ELSE regexp_replace(\
                             upper(encode(convert_to(chr, 'UTF8'), 'hex')), \
                             '(..)', '%\\1', 'g') \
                    END, \
                    '' ORDER BY pos) \
                 FROM regexp_split_to_table({text}, '') WITH ORDINALITY AS t(chr, pos))"
            )))
        }

        // ── STRLANG ─────────────────────────────────────────────────────────
        // STRLANG(?str, ?lang) → encode as language-tagged literal.
        // Type error: input must be a plain literal (kind=2) or xsd:string typed literal.
        // Lang-tagged (kind=4), IRI (kind=0), other typed literals, or inline → NULL.
        Function::StrLang => {
            let lang_col = translate_arg_value(args.get(1)?, bindings, ctx)?;
            let lang_text = decode_lexical_sql(&lang_col);
            // Fast path: STR(?x) always returns a plain literal, no type check needed.
            if let Some(Expression::FunctionCall(Function::Str, str_args)) = args.first() {
                let inner_col = translate_arg_value(str_args.first()?, bindings, ctx)?;
                let str_text = decode_lexical_sql(&inner_col);
                // Preserve lang tag case as-is (SPARQL spec does not normalize lang tags).
                return Some(format!(
                    "pg_ripple.encode_lang_literal({str_text}, {lang_text})"
                ));
            }
            let str_col = translate_arg_value(args.first()?, bindings, ctx)?;
            let str_text = decode_lexical_sql(&str_col);
            Some(format!(
                "CASE \
                   WHEN {str_col} < 0 THEN NULL \
                   WHEN NOT EXISTS(SELECT 1 FROM _pg_ripple.dictionary _dc WHERE _dc.id = {str_col} \
                       AND (_dc.kind = 2 OR (_dc.kind = 3 AND _dc.datatype = \
                           'http://www.w3.org/2001/XMLSchema#string'))) THEN NULL \
                   ELSE pg_ripple.encode_lang_literal({str_text}, {lang_text}) \
                 END"
            ))
        }

        // ── STRDT ───────────────────────────────────────────────────────────
        // STRDT(?str, ?datatype) → encode as typed literal with given datatype.
        // Type error: input must be a plain literal (kind=2) or xsd:string typed literal.
        // Lang-tagged, IRI, other typed literals, or inline → NULL.
        Function::StrDt => {
            let dt_arg = args.get(1)?;
            // Extract the datatype IRI text. Named node IRI → use the IRI string directly.
            let dt_text = match dt_arg {
                Expression::NamedNode(nn) => format!("'{}'", nn.as_str().replace('\'', "''")),
                _ => {
                    let dt_col = translate_arg_value(dt_arg, bindings, ctx)?;
                    decode_lexical_sql(&dt_col)
                }
            };
            // Fast path: STR(?x) always returns a plain literal, no type check needed.
            if let Some(Expression::FunctionCall(Function::Str, str_args)) = args.first() {
                let inner_col = translate_arg_value(str_args.first()?, bindings, ctx)?;
                let str_text = decode_lexical_sql(&inner_col);
                return Some(format!(
                    "pg_ripple.encode_typed_literal({str_text}, {dt_text})"
                ));
            }
            let str_col = translate_arg_value(args.first()?, bindings, ctx)?;
            let str_text = decode_lexical_sql(&str_col);
            Some(format!(
                "CASE \
                   WHEN {str_col} < 0 THEN NULL \
                   WHEN NOT EXISTS(SELECT 1 FROM _pg_ripple.dictionary _dc WHERE _dc.id = {str_col} \
                       AND (_dc.kind = 2 OR (_dc.kind = 3 AND _dc.datatype = \
                           'http://www.w3.org/2001/XMLSchema#string'))) THEN NULL \
                   ELSE pg_ripple.encode_typed_literal({str_text}, {dt_text}) \
                 END"
            ))
        }

        // ── IRI / URI ────────────────────────────────────────────────────────
        Function::Iri => {
            // When the argument is a NamedNode (IRI), return it directly.
            // For string literals, decode and re-encode (with optional BASE resolution).
            if let Some(Expression::NamedNode(nn)) = args.first() {
                // The argument is already a resolved IRI. Encode it directly.
                let iri = nn.as_str();
                if let Some(id) = ctx.encode_iri(iri) {
                    return Some(id.to_string());
                }
                // IRI not yet in dictionary — use runtime insert/lookup.
                let iri_esc = iri.replace('\'', "''");
                return Some(format!("pg_ripple.encode_term('{iri_esc}', 0::int2)"));
            }
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            // If there is a BASE IRI, resolve relative IRIs at runtime using SQL.
            // We detect relative IRIs by checking that the value does NOT start with
            // a scheme (i.e. no colon before the first slash or end of string).
            if let Some(base) = &ctx.base_iri.clone() {
                let base_escaped = base.replace('\'', "''");
                // Emit SQL that resolves relative IRIs: if the value already contains
                // '://' or starts with '<', use it as-is; otherwise prepend the base.
                Some(encode_iri(format!(
                    "(CASE WHEN ({text}) ~ '^[A-Za-z][A-Za-z0-9+\\-.]*:' \
                          THEN ({text}) \
                          ELSE '{base_escaped}' || ({text}) \
                     END)"
                )))
            } else {
                Some(encode_iri(text))
            }
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
            // raw_double_vars (RAND() results) are always xsd:double — shortcut without
            // dict lookup (which would fail for raw floats and snapshot isolation).
            if let Some(Expression::Variable(v)) = args.first() {
                if ctx.is_raw_double_var(v.as_str()) {
                    return Some(encode_iri(
                        "'http://www.w3.org/2001/XMLSchema#double'".to_owned(),
                    ));
                }
            }
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            // For inline IDs (negative): extract type code from bits 62-56.
            // Mask 0x7F00000000000000 = 9151314442816847872; shift >> 56 gives type code.
            // TYPE_INTEGER=0 → xsd:integer, TYPE_BOOLEAN=1 → xsd:boolean,
            // TYPE_DATETIME=2 → xsd:dateTime, TYPE_DATE=3 → xsd:date.
            // For dictionary-resident IDs: only literals (kind 2,3,4) have a datatype;
            // IRIs (kind=0) and blank nodes (kind=1) produce a type error (NULL).
            Some(encode_iri(format!(
                "CASE \
                   WHEN {col} IS NULL THEN NULL \
                   WHEN {col} < 0 THEN \
                     CASE (({col} & 9151314442816847872::bigint) >> 56) \
                       WHEN 0 THEN 'http://www.w3.org/2001/XMLSchema#integer' \
                       WHEN 1 THEN 'http://www.w3.org/2001/XMLSchema#boolean' \
                       WHEN 2 THEN 'http://www.w3.org/2001/XMLSchema#dateTime' \
                       WHEN 3 THEN 'http://www.w3.org/2001/XMLSchema#date' \
                       ELSE 'http://www.w3.org/2001/XMLSchema#integer' \
                     END \
                   ELSE (\
                     SELECT CASE d.kind \
                       WHEN 3 THEN d.datatype \
                       WHEN 2 THEN 'http://www.w3.org/2001/XMLSchema#string' \
                       WHEN 4 THEN 'http://www.w3.org/1999/02/22-rdf-syntax-ns#langString' \
                       ELSE NULL \
                     END \
                     FROM _pg_ripple.dictionary d WHERE d.id = {col}\
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
            // Return typed literal preserving input type (xsd:decimal → xsd:decimal, etc.)
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(format!(
                "pg_ripple.encode_typed_literal(\
                    ceil(({text})::numeric)::text, \
                    CASE WHEN {col} < 0 THEN 'http://www.w3.org/2001/XMLSchema#integer' \
                         ELSE COALESCE(\
                             (SELECT d.datatype FROM _pg_ripple.dictionary d WHERE d.id = {col} AND d.kind = 3), \
                             'http://www.w3.org/2001/XMLSchema#integer') \
                    END)"
            ))
        }
        Function::Floor => {
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(format!(
                "pg_ripple.encode_typed_literal(\
                    floor(({text})::numeric)::text, \
                    CASE WHEN {col} < 0 THEN 'http://www.w3.org/2001/XMLSchema#integer' \
                         ELSE COALESCE(\
                             (SELECT d.datatype FROM _pg_ripple.dictionary d WHERE d.id = {col} AND d.kind = 3), \
                             'http://www.w3.org/2001/XMLSchema#integer') \
                    END)"
            ))
        }
        Function::Round => {
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(format!(
                "pg_ripple.encode_typed_literal(\
                    round(({text})::numeric)::text, \
                    CASE WHEN {col} < 0 THEN 'http://www.w3.org/2001/XMLSchema#integer' \
                         ELSE COALESCE(\
                             (SELECT d.datatype FROM _pg_ripple.dictionary d WHERE d.id = {col} AND d.kind = 3), \
                             'http://www.w3.org/2001/XMLSchema#integer') \
                    END)"
            ))
        }
        Function::Rand => {
            // RAND() → raw double in [0, 1). Raw float, not dict-encoded.
            // Marked is_numeric so comparisons (>= 0.0, < 1.0) work directly.
            // DATATYPE() is handled via raw_double_vars tracking (returns xsd:double).
            *is_numeric = true;
            Some("random()".to_owned())
        }

        // ── Datetime functions ───────────────────────────────────────────────
        Function::Now => {
            // NOW() → encode current timestamp as xsd:dateTime typed literal.
            Some(format!(
                "pg_ripple.encode_typed_literal(\
                    to_char(now(), 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"'), \
                    'http://www.w3.org/2001/XMLSchema#dateTime')"
            ))
        }
        Function::Year => {
            *is_numeric = true;
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            // Parse year directly from ISO 8601 string to avoid timezone conversion.
            Some(format!("(substring({text} FROM '^(\\d{{4}})-'))::bigint"))
        }
        Function::Month => {
            *is_numeric = true;
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(format!(
                "(substring({text} FROM '^\\d{{4}}-(\\d{{2}})-'))::bigint"
            ))
        }
        Function::Day => {
            *is_numeric = true;
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(format!(
                "(substring({text} FROM '^\\d{{4}}-\\d{{2}}-(\\d{{2}})T'))::bigint"
            ))
        }
        Function::Hours => {
            *is_numeric = true;
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(format!("(substring({text} FROM 'T(\\d{{2}}):'))::bigint"))
        }
        Function::Minutes => {
            *is_numeric = true;
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            Some(format!(
                "(substring({text} FROM 'T\\d{{2}}:(\\d{{2}}):'))::bigint"
            ))
        }
        Function::Seconds => {
            // SPARQL spec: SECONDS returns xsd:decimal (not xsd:integer).
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            // Extract seconds (integer or fractional) and encode as xsd:decimal.
            // Inline datetimes store UTC with microseconds (e.g. "01.000000"), so
            // strip leading zeros via ::numeric and trailing fractional zeros via RTRIM.
            Some(format!(
                "pg_ripple.encode_typed_literal(\
                    RTRIM(RTRIM((COALESCE(substring({text} FROM 'T\\d{{2}}:\\d{{2}}:(\\d+(?:\\.\\d+)?)'), '0'))::numeric::text, '0'), '.'), \
                    'http://www.w3.org/2001/XMLSchema#decimal')"
            ))
        }
        Function::Timezone => {
            // Returns the timezone offset as xsd:dayTimeDuration (e.g. "PT0S", "-PT8H").
            // Inline datetimes are stored in UTC, so timezone is always Z → "PT0S".
            // For dict-stored datetimes with explicit timezone, extract and convert.
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            // Convert timezone string to dayTimeDuration:
            //   "Z" or "+00:00" or "-00:00" → "PT0S"
            //   "+HH:MM" → "PTHHS" (ignoring minutes for common cases)
            //   "-HH:MM" → "-PTHHS"
            let tz_expr = format!(
                "CASE \
                   WHEN ({text}) LIKE '%Z' THEN 'PT0S' \
                   WHEN ({text}) ~ '[+-]\\d{{2}}:\\d{{2}}$' THEN (\
                     WITH tz AS (SELECT substring(({text}) from '[+-]\\d{{2}}:\\d{{2}}$') AS t) \
                     SELECT CASE \
                       WHEN t = '+00:00' OR t = '-00:00' THEN 'PT0S' \
                       WHEN left(t,1) = '-' THEN '-PT' || ltrim(substring(t from 2 for 2),'0') || 'H' \
                       ELSE 'PT' || ltrim(substring(t from 2 for 2),'0') || 'H' \
                     END FROM tz) \
                   ELSE NULL \
                 END"
            );
            Some(format!(
                "pg_ripple.encode_typed_literal(\
                    ({tz_expr}), \
                    'http://www.w3.org/2001/XMLSchema#dayTimeDuration')"
            ))
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
            // Use pgcrypto digest() for SHA1 (requires pgcrypto extension).
            Some(encode_literal(format!(
                "encode(digest(({text})::bytea, 'sha1'), 'hex')"
            )))
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
            // Use pgcrypto digest() for SHA384.
            Some(encode_literal(format!(
                "encode(digest(({text})::bytea, 'sha384'), 'hex')"
            )))
        }
        Function::Sha512 => {
            let col = translate_arg_value(args.first()?, bindings, ctx)?;
            let text = decode_lexical_sql(&col);
            // Use pgcrypto digest() for SHA512.
            Some(encode_literal(format!(
                "encode(digest(({text})::bytea, 'sha512'), 'hex')"
            )))
        }

        // ── UUID / STRUUID ────────────────────────────────────────────────────
        // UUID() / STRUUID() are volatile: encode_term inserts a new dict row but
        // the same-statement snapshot can't see it, so ISIRI/ISLITERAL/REGEX all
        // fail via the normal dict-lookup path.  Instead, return raw text and track
        // the variable via raw_iri_vars / raw_text_vars so callers shortcut checks.
        Function::Uuid => {
            // UUID() → raw IRI text; caller marks variable in raw_iri_vars.
            Some("('urn:uuid:' || gen_random_uuid()::text)".to_owned())
        }
        Function::StrUuid => {
            // STRUUID() → raw UUID text; caller marks variable in raw_text_vars.
            Some("gen_random_uuid()::text".to_owned())
        }

        // ── STRBEFORE / STRAFTER ─────────────────────────────────────────────
        // Preserve the language tag of the subject literal when needle is found.
        // When needle is not found: return "" (plain literal).
        // When input is inline (integer/boolean/datetime): return NULL (type error).
        Function::StrBefore => {
            let str_col = translate_arg_value(args.first()?, bindings, ctx)?;
            let str_text = decode_lexical_sql(&str_col);
            let needle_expr = args.get(1)?;

            // Determine needle SQL text, compatibility check, and empty-needle result.
            // For lang-tagged needles: input must have same lang tag, else type error (NULL).
            // For plain/xsd:string needles: always compatible.
            let (needle_sql, compat_sql, empty_result_sql) =
                if let Expression::Literal(lit) = needle_expr {
                    let val = lit.value().replace('\'', "''");
                    let needle_sql = format!("'{val}'");
                    if let Some(lang) = lit.language() {
                        let lang_lower = lang.to_lowercase();
                        // Compatible only if input is lang-tagged with same lang.
                        let compat = format!(
                            "{str_col} > 0 AND EXISTS(\
                             SELECT 1 FROM _pg_ripple.dictionary _dn \
                             WHERE _dn.id = {str_col} AND _dn.kind = 4 \
                             AND LOWER(_dn.lang) = '{lang_lower}')"
                        );
                        // Empty lang-tagged needle: return ""@lang.
                        let empty_res = format!(
                            "(SELECT pg_ripple.encode_lang_literal('', _dl.lang) \
                              FROM _pg_ripple.dictionary _dl WHERE _dl.id = {str_col})"
                        );
                        (needle_sql, compat, empty_res)
                    } else {
                        // Plain / xsd:string needle — always compatible.
                        let compat = "TRUE".to_string();
                        let empty_res = encode_preserving_lang(&str_col, "''");
                        (needle_sql, compat, empty_res)
                    }
                } else {
                    // Variable/complex needle — treat as plain (no lang check).
                    let t = translate_arg_text(needle_expr, bindings, ctx)?;
                    let compat = "TRUE".to_string();
                    let empty_res = encode_preserving_lang(&str_col, "''");
                    (t, compat, empty_res)
                };

            let found_expr = encode_preserving_lang(
                &str_col,
                &format!("left({str_text}, strpos({str_text}, {needle_sql}) - 1)"),
            );
            Some(format!(
                "CASE \
                   WHEN {str_col} < 0 THEN NULL \
                   WHEN NOT ({compat_sql}) THEN NULL \
                   WHEN {needle_sql} = '' THEN {empty_result_sql} \
                   WHEN strpos({str_text}, {needle_sql}) > 0 THEN {found_expr} \
                   ELSE pg_ripple.encode_term('', 2::int2) \
                 END"
            ))
        }
        Function::StrAfter => {
            let str_col = translate_arg_value(args.first()?, bindings, ctx)?;
            let str_text = decode_lexical_sql(&str_col);
            let needle_expr = args.get(1)?;

            let (needle_sql, compat_sql, empty_result_sql) =
                if let Expression::Literal(lit) = needle_expr {
                    let val = lit.value().replace('\'', "''");
                    let needle_sql = format!("'{val}'");
                    if let Some(lang) = lit.language() {
                        let lang_lower = lang.to_lowercase();
                        let compat = format!(
                            "{str_col} > 0 AND EXISTS(\
                             SELECT 1 FROM _pg_ripple.dictionary _dn \
                             WHERE _dn.id = {str_col} AND _dn.kind = 4 \
                             AND LOWER(_dn.lang) = '{lang_lower}')"
                        );
                        // Empty lang-tagged needle: return full string @lang.
                        let empty_res = encode_preserving_lang(&str_col, &str_text);
                        (needle_sql, compat, empty_res)
                    } else {
                        let compat = "TRUE".to_string();
                        let empty_res = encode_preserving_lang(&str_col, &str_text);
                        (needle_sql, compat, empty_res)
                    }
                } else {
                    let t = translate_arg_text(needle_expr, bindings, ctx)?;
                    let compat = "TRUE".to_string();
                    let empty_res = encode_preserving_lang(&str_col, &str_text);
                    (t, compat, empty_res)
                };

            let found_expr = encode_preserving_lang(
                &str_col,
                &format!(
                    "right({str_text}, length({str_text}) - strpos({str_text}, {needle_sql}) - length({needle_sql}) + 1)"
                ),
            );
            Some(format!(
                "CASE \
                   WHEN {str_col} < 0 THEN NULL \
                   WHEN NOT ({compat_sql}) THEN NULL \
                   WHEN {needle_sql} = '' THEN {empty_result_sql} \
                   WHEN strpos({str_text}, {needle_sql}) > 0 THEN {found_expr} \
                   ELSE pg_ripple.encode_term('', 2::int2) \
                 END"
            ))
        }

        // ── COALESCE ─────────────────────────────────────────────────────────
        // Note: COALESCE is Expression::Coalesce in spargebra, not a Function.
        // This arm is unreachable but kept for completeness.

        // ── RDF-star functions ────────────────────────────────────────────────
        // These are behind the sparql-12 feature flag; return None for now.

        // ── GeoSPARQL non-topological functions ───────────────────────────
        // geof:distance, geof:area, geof:boundary — return numeric / WKT literals.
        // PostGIS availability is probed at translation time; when PostGIS is
        // absent, NULL is emitted without any PostGIS function reference.
        Function::Custom(name) => {
            let iri = name.as_str();

            // ── XSD type cast functions ─────────────────────────────────────
            // xsd:integer(?v), xsd:decimal(?v), xsd:double(?v), etc.
            // These are SPARQL 1.1 §17.1 constructor functions.
            if let Some(dt) = xsd_cast_datatype(iri) {
                let arg_col = translate_arg_value(args.first()?, bindings, ctx)?;
                return Some(xsd_cast_sql(&arg_col, dt));
            }

            match iri {
                "http://www.opengis.net/def/function/geosparql/distance" => {
                    // geof:distance(?a, ?b, unit) → numeric distance (metres for unit-of-measure)
                    *is_numeric = true;
                    if !postgis_available() {
                        return Some("NULL".to_string());
                    }
                    let a_col = translate_arg_value(args.first()?, bindings, ctx)?;
                    let b_col = translate_arg_value(args.get(1)?, bindings, ctx)?;
                    let a_wkt = decode_lexical_sql(&a_col);
                    let b_wkt = decode_lexical_sql(&b_col);
                    Some(format!(
                        "ST_Distance(\
                            ST_GeomFromText({a_wkt})::geography, \
                            ST_GeomFromText({b_wkt})::geography\
                          )"
                    ))
                }
                "http://www.opengis.net/def/function/geosparql/area" => {
                    *is_numeric = true;
                    if !postgis_available() {
                        return Some("NULL".to_string());
                    }
                    let a_col = translate_arg_value(args.first()?, bindings, ctx)?;
                    let a_wkt = decode_lexical_sql(&a_col);
                    Some(format!("ST_Area(ST_GeomFromText({a_wkt})::geography)"))
                }
                "http://www.opengis.net/def/function/geosparql/boundary" => {
                    // Returns a WKT literal of the boundary geometry.
                    if !postgis_available() {
                        return Some(encode_literal("NULL".to_string()));
                    }
                    let a_col = translate_arg_value(args.first()?, bindings, ctx)?;
                    let a_wkt = decode_lexical_sql(&a_col);
                    Some(encode_literal(format!(
                        "ST_AsText(ST_Boundary(ST_GeomFromText({a_wkt})))"
                    )))
                }

                // ── pg:similar(?entity, "text", k) — pgvector cosine distance ──
                // Returns cosine distance as xsd:double (0 = identical, 2 = opposite).
                // When pgvector is absent or disabled, emits NULL::float8.
                PG_SIMILAR_IRI => {
                    *is_numeric = true;
                    let entity_col = translate_arg_value(args.first()?, bindings, ctx)?;
                    let query_text = args
                        .get(1)
                        .and_then(|e| {
                            if let Expression::Literal(lit) = e {
                                Some(lit.value().to_owned())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_default();
                    let k = args
                        .get(2)
                        .and_then(|e| {
                            if let Expression::Literal(lit) = e {
                                lit.value().parse::<i64>().ok()
                            } else {
                                None
                            }
                        })
                        .unwrap_or(10);
                    Some(crate::sparql::embedding::sql_for_pg_similar(
                        &entity_col,
                        &query_text,
                        k,
                    ))
                }

                _ => None,
            }
        }

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
            | Function::Rand
            | Function::Year
            | Function::Month
            | Function::Day
            | Function::Hours
            | Function::Minutes // CEIL, FLOOR, ROUND, SECONDS now return typed literal dict IDs, not raw numerics.
    )
}

// ─── XSD type cast helpers ─────────────────────────────────────────────────────

/// Return the full XSD datatype IRI if `iri` is a SPARQL 1.1 constructor
/// (e.g. `xsd:integer`) that we support, otherwise `None`.
fn xsd_cast_datatype(iri: &str) -> Option<&'static str> {
    match iri {
        "http://www.w3.org/2001/XMLSchema#integer" => {
            Some("http://www.w3.org/2001/XMLSchema#integer")
        }
        "http://www.w3.org/2001/XMLSchema#decimal" => {
            Some("http://www.w3.org/2001/XMLSchema#decimal")
        }
        "http://www.w3.org/2001/XMLSchema#double" => {
            Some("http://www.w3.org/2001/XMLSchema#double")
        }
        "http://www.w3.org/2001/XMLSchema#float" => Some("http://www.w3.org/2001/XMLSchema#float"),
        "http://www.w3.org/2001/XMLSchema#string" => {
            Some("http://www.w3.org/2001/XMLSchema#string")
        }
        "http://www.w3.org/2001/XMLSchema#boolean" => {
            Some("http://www.w3.org/2001/XMLSchema#boolean")
        }
        "http://www.w3.org/2001/XMLSchema#dateTime" => {
            Some("http://www.w3.org/2001/XMLSchema#dateTime")
        }
        _ => None,
    }
}

/// Build SQL that casts the encoded bigint `col` to `dt` and re-encodes.
/// Returns NULL on cast failure.
fn xsd_cast_sql(col: &str, dt: &str) -> String {
    // Decode column to its lexical string form.
    let lex = format!(
        "CASE WHEN ({col}) IS NULL THEN NULL \
         WHEN ({col}) < 0 THEN \
           ((({col}) & 72057594037927935::bigint) - 36028797018963968::bigint)::text \
         ELSE (SELECT d.value FROM _pg_ripple.dictionary d WHERE d.id = ({col}) LIMIT 1) \
         END"
    );
    // Numeric pattern: optional sign, digits with optional decimal point, optional exponent.
    let num_re = r"^[+\-]?(\d+\.?\d*|\d*\.\d+)([eE][+\-]?\d+)?$";
    match dt {
        "http://www.w3.org/2001/XMLSchema#integer" => {
            // Truncate to integer (floor toward zero). Use regex to guard against bad input.
            format!(
                "CASE WHEN ({lex}) IS NULL OR ({lex}) !~ '{num_re}' THEN NULL \
                 ELSE pg_ripple.encode_typed_literal(\
                   trunc(({lex})::numeric)::bigint::text, \
                   'http://www.w3.org/2001/XMLSchema#integer') END"
            )
        }
        "http://www.w3.org/2001/XMLSchema#decimal" => {
            format!(
                "CASE WHEN ({lex}) IS NULL OR ({lex}) !~ '{num_re}' THEN NULL \
                 ELSE pg_ripple.encode_typed_literal(\
                   trim_scale(({lex})::numeric)::text, \
                   'http://www.w3.org/2001/XMLSchema#decimal') END"
            )
        }
        "http://www.w3.org/2001/XMLSchema#double" | "http://www.w3.org/2001/XMLSchema#float" => {
            format!(
                "CASE WHEN ({lex}) IS NULL OR ({lex}) !~ '{num_re}' THEN NULL \
                 ELSE pg_ripple.encode_typed_literal(\
                   pg_ripple.xsd_double_fmt(({lex})::float8::text), \
                   'http://www.w3.org/2001/XMLSchema#double') END"
            )
        }
        "http://www.w3.org/2001/XMLSchema#string" => {
            // Re-encode the lexical value as xsd:string (= plain literal in RDF 1.1).
            format!(
                "pg_ripple.encode_typed_literal(\
                   {lex}, \
                   'http://www.w3.org/2001/XMLSchema#string')"
            )
        }
        "http://www.w3.org/2001/XMLSchema#boolean" => {
            format!(
                "pg_ripple.encode_typed_literal(\
                   CASE WHEN lower({lex}) IN ('true','1') THEN 'true' \
                        WHEN lower({lex}) IN ('false','0') THEN 'false' \
                        ELSE NULL END, \
                   'http://www.w3.org/2001/XMLSchema#boolean')"
            )
        }
        "http://www.w3.org/2001/XMLSchema#dateTime" => {
            format!(
                "pg_ripple.encode_typed_literal(\
                   ({lex})::timestamptz::text, \
                   'http://www.w3.org/2001/XMLSchema#dateTime')"
            )
        }
        _ => "NULL".to_string(),
    }
}
