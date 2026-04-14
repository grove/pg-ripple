//! Export — serialize stored triples to N-Triples and N-Quads format.
//!
//! Queries all VP tables (dedicated + vp_rare) for the requested graph(s),
//! decodes the integer IDs in bulk via `dictionary::format_ntriples`, and
//! assembles an N-Triples or N-Quads text document.
//!
//! Streaming variants returning `SETOF TEXT` (one line per triple) will be
//! added in v0.9.0 when the full serialization milestone is delivered.

use crate::{dictionary, storage};

// ─── N-Triples ────────────────────────────────────────────────────────────────

/// Export triples as N-Triples text.
///
/// If `graph` is `None`, export the default graph (g = 0).
/// N-Triples format does not include graph information.
pub fn export_ntriples(graph: Option<&str>) -> String {
    let g_id: Option<i64> = match graph {
        Some(g_str) => {
            let stripped = if g_str.starts_with('<') && g_str.ends_with('>') {
                &g_str[1..g_str.len() - 1]
            } else {
                g_str
            };
            Some(crate::dictionary::encode(
                stripped,
                crate::dictionary::KIND_IRI,
            ))
        }
        None => Some(0), // default graph
    };

    let rows = storage::all_encoded_triples(g_id);

    let mut out = String::with_capacity(rows.len() * 80);
    for (s_id, p_id, o_id, _g) in &rows {
        let s = dictionary::format_ntriples(*s_id);
        let p = dictionary::format_ntriples(*p_id);
        let o = dictionary::format_ntriples(*o_id);
        out.push_str(&s);
        out.push(' ');
        out.push_str(&p);
        out.push(' ');
        out.push_str(&o);
        out.push_str(" .\n");
    }
    out
}

// ─── N-Quads ─────────────────────────────────────────────────────────────────

/// Export triples as N-Quads text.
///
/// If `graph` is `None`, all graphs are exported.  A graph-column value of 0
/// (default graph) is omitted from the quad line (yielding a triple-like line).
/// A named-graph value is included as the fourth field.
pub fn export_nquads(graph: Option<&str>) -> String {
    let g_filter: Option<i64> = match graph {
        Some(g_str) => {
            let stripped = if g_str.starts_with('<') && g_str.ends_with('>') {
                &g_str[1..g_str.len() - 1]
            } else {
                g_str
            };
            Some(crate::dictionary::encode(
                stripped,
                crate::dictionary::KIND_IRI,
            ))
        }
        None => None, // all graphs
    };

    let rows = storage::all_encoded_triples(g_filter);

    let mut out = String::with_capacity(rows.len() * 100);
    for (s_id, p_id, o_id, g_id) in &rows {
        let s = dictionary::format_ntriples(*s_id);
        let p = dictionary::format_ntriples(*p_id);
        let o = dictionary::format_ntriples(*o_id);
        out.push_str(&s);
        out.push(' ');
        out.push_str(&p);
        out.push(' ');
        out.push_str(&o);
        if *g_id > 0 {
            out.push(' ');
            out.push_str(&dictionary::format_ntriples(*g_id));
        }
        out.push_str(" .\n");
    }
    out
}
