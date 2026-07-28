//! SHACL validation engine — focus-node collection, constraint dispatch,
//! synchronous validation, and the async validation batch processor.

use serde::Serialize;

// ─── Violation record ─────────────────────────────────────────────────────────

/// A violation entry in a SHACL validation report.
#[derive(Debug, Serialize)]
pub struct Violation {
    pub focus_node: String,
    pub shape_iri: String,
    pub path: Option<String>,
    pub constraint: String,
    pub message: String,
    pub severity: String,
    /// The offending value node, decoded (v0.48.0, W3C `sh:value`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sh_value: Option<String>,
    /// W3C constraint component IRI (v0.48.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sh_source_constraint_component: Option<String>,
}

// ─── Falha de validação de escrita ───────────────────────────────────────────

/// Uma violação encontrada ao validar uma única afirmação (modo `sync` ou
/// `async`), com a shape e a restrição que a produziram.
///
/// Antes isso era uma `String` solta. O consumidor do modo `async` gravava
/// `"shapeIRI": "unknown"` em toda linha da fila de descarte, o que deixava o
/// índice por shape e o resumo por restrição sem nada para agrupar: 18 mil
/// linhas indistinguíveis. Aqui a origem vem junto.
#[derive(Debug, Serialize)]
pub struct SyncViolation {
    pub shape_iri: String,
    pub path: Option<String>,
    pub constraint: String,
    pub message: String,
}

impl SyncViolation {
    pub(crate) fn new(
        shape_iri: &str,
        path: &str,
        constraint: &str,
        message: String,
    ) -> SyncViolation {
        SyncViolation {
            shape_iri: shape_iri.to_owned(),
            path: Some(path.to_owned()),
            constraint: constraint.to_owned(),
            message,
        }
    }
}

impl std::fmt::Display for SyncViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

// ─── Recursive shape conformance ─────────────────────────────────────────────
