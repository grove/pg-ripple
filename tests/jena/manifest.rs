//! Apache Jena test manifest parser.
//!
//! Extends the W3C manifest vocabulary with Jena-specific test type IRIs.
//! The manifest structure (Turtle RDF, `mf:entries` list, `mf:action` / `mf:result`)
//! is identical to W3C — only the type IRIs differ.
//!
//! # Supported test types
//!
//! | IRI | Meaning |
//! |-----|---------|
//! | `mf:QueryEvaluationTest` | W3C query evaluation (reused) |
//! | `jt:QueryEvaluationTest` | Jena query evaluation |
//! | `mf:UpdateEvaluationTest` | W3C update evaluation (reused) |
//! | `jt:UpdateEvaluationTest` | Jena update evaluation |
//! | `jt:NegativeSyntaxTest` | Query must fail to parse |
//! | `jt:PositiveSyntaxTest` | Query must parse without error |
//! | `mf:PositiveSyntaxTest11` | W3C positive syntax (reused) |
//! | `mf:NegativeSyntaxTest11` | W3C negative syntax (reused) |

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rio_api::model::{Literal, Subject, Term};
use rio_api::parser::TriplesParser;
use rio_turtle::{TurtleError, TurtleParser};

// ── Jena-specific IRIs ────────────────────────────────────────────────────────

const JT_QUERY_EVAL: &str =
    "http://jena.hpl.hp.com/2005/05/test-manifest-extra#QueryEvaluationTest";
const JT_UPDATE_EVAL: &str =
    "http://jena.hpl.hp.com/2005/05/test-manifest-extra#UpdateEvaluationTest";
const JT_NEG_SYNTAX: &str = "http://jena.hpl.hp.com/2005/05/test-manifest-extra#NegativeSyntaxTest";
const JT_POS_SYNTAX: &str = "http://jena.hpl.hp.com/2005/05/test-manifest-extra#PositiveSyntaxTest";

// ── Shared W3C IRIs ───────────────────────────────────────────────────────────

const MF_MANIFEST: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#Manifest";
const MF_ENTRIES: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#entries";
const MF_NAME: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#name";
const MF_ACTION: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#action";
const MF_RESULT: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#result";
const MF_QUERY_EVAL: &str =
    "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#QueryEvaluationTest";
const MF_UPDATE_EVAL: &str =
    "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#UpdateEvaluationTest";
const UT_UPDATE_EVAL: &str = "http://www.w3.org/2009/sparql/tests/test-update#UpdateEvaluationTest";
const MF_POS_SYNTAX: &str =
    "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#PositiveSyntaxTest11";
const MF_NEG_SYNTAX: &str =
    "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#NegativeSyntaxTest11";
const QT_QUERY: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-query#query";
const QT_DATA: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-query#data";
const QT_GRAPH_DATA: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-query#graphData";
const UT_REQUEST: &str = "http://www.w3.org/2009/sparql/tests/test-update#request";
const UT_DATA: &str = "http://www.w3.org/2009/sparql/tests/test-update#data";
const UT_GRAPH_DATA: &str = "http://www.w3.org/2009/sparql/tests/test-update#graphData";
const UT_GRAPH: &str = "http://www.w3.org/2009/sparql/tests/test-update#graph";
const UT_GRAPH_IRI: &str = "http://www.w3.org/2009/sparql/tests/test-update#graphIRI";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

// ── Public types ──────────────────────────────────────────────────────────────

/// A single Jena test case.
#[derive(Debug, Clone)]
pub struct JenaTestCase {
    /// The test IRI (manifest subject node).
    pub iri: String,
    /// Human-readable name.
    pub name: String,
    /// The test type.
    pub test_type: JenaTestType,
    /// Sub-suite category (e.g. `"sparql-query"`).
    pub category: String,
    /// SPARQL query or update file.
    pub query_file: Option<PathBuf>,
    /// Default-graph data files.
    pub data_files: Vec<PathBuf>,
    /// Named-graph files: `(graph IRI, file path)`.
    pub named_graphs: Vec<(String, PathBuf)>,
    /// Expected result file.
    pub result_file: Option<PathBuf>,
    /// Expected result data files for UPDATE tests.
    pub update_result_data: Vec<PathBuf>,
    /// Expected named graph result files for UPDATE tests.
    pub update_result_graphs: Vec<(String, PathBuf)>,
}

/// Jena test types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JenaTestType {
    /// Run a SPARQL query and compare results.
    QueryEvaluation,
    /// Run a SPARQL UPDATE and compare resulting graph.
    UpdateEvaluation,
    /// Query must parse without error.
    PositiveSyntax,
    /// Query must fail to parse.
    NegativeSyntax,
    /// Unrecognised type — skip.
    NotClassified,
}

/// Parse all Jena test cases from a Turtle manifest file.
pub fn parse_manifest(
    manifest_path: &Path,
    category: &str,
) -> Result<Vec<JenaTestCase>, Box<dyn std::error::Error + Send + Sync>> {
    let src = std::fs::read_to_string(manifest_path)
        .map_err(|e| format!("reading manifest {}: {e}", manifest_path.display()))?;

    let base = format!(
        "file://{}",
        manifest_path
            .canonicalize()
            .unwrap_or_else(|_| manifest_path.to_path_buf())
            .display()
    );

    // Collect all triples.
    let mut triples: Vec<(String, String, String)> = Vec::new();
    let mut parser = TurtleParser::new(src.as_bytes(), None);

    parser
        .parse_all(&mut |t| -> Result<(), TurtleError> {
            let s = subject_to_string(t.subject);
            let p = t.predicate.iri.to_string();
            let o = term_to_string(t.object);
            triples.push((s, p, o));
            Ok(())
        })
        .ok(); // Best-effort; some Jena manifests use extensions that rio doesn't understand.

    // Index triples by subject.
    let mut by_subj: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for (s, p, o) in &triples {
        by_subj
            .entry(s.clone())
            .or_default()
            .push((p.clone(), o.clone()));
    }

    // Find manifest node.
    let manifest_node = triples
        .iter()
        .find(|(_, p, o)| p == RDF_TYPE && o == MF_MANIFEST)
        .map(|(s, _, _)| s.clone())
        .unwrap_or_else(|| base.clone());

    // Build rdf:list to ordered Vec.
    let entries_list_head = by_subj
        .get(&manifest_node)
        .and_then(|props| props.iter().find(|(p, _)| p == MF_ENTRIES))
        .map(|(_, o)| o.clone())
        .unwrap_or_else(|| RDF_NIL.to_string());

    let entry_iris = collect_list(&entries_list_head, &by_subj);

    let mut cases = Vec::new();
    for iri in entry_iris {
        let props = match by_subj.get(&iri) {
            Some(p) => p,
            None => continue,
        };

        let name = props
            .iter()
            .find(|(p, _)| p == MF_NAME)
            .map(|(_, o)| strip_literal(o))
            .unwrap_or_else(|| iri.clone());

        let type_iri = props
            .iter()
            .find(|(p, _)| p == RDF_TYPE)
            .map(|(_, o)| o.as_str())
            .unwrap_or("");

        let test_type = match type_iri {
            t if t == MF_QUERY_EVAL || t == JT_QUERY_EVAL => JenaTestType::QueryEvaluation,
            t if t == MF_UPDATE_EVAL || t == UT_UPDATE_EVAL || t == JT_UPDATE_EVAL => {
                JenaTestType::UpdateEvaluation
            }
            t if t == MF_POS_SYNTAX || t == JT_POS_SYNTAX => JenaTestType::PositiveSyntax,
            t if t == MF_NEG_SYNTAX || t == JT_NEG_SYNTAX => JenaTestType::NegativeSyntax,
            _ => JenaTestType::NotClassified,
        };

        let action_node = props
            .iter()
            .find(|(p, _)| p == MF_ACTION)
            .map(|(_, o)| o.clone());

        let result_node = props
            .iter()
            .find(|(p, _)| p == MF_RESULT)
            .map(|(_, o)| o.clone());

        let mut query_file: Option<PathBuf> = None;
        let mut data_files: Vec<PathBuf> = Vec::new();
        let mut named_graphs: Vec<(String, PathBuf)> = Vec::new();
        let mut update_result_data: Vec<PathBuf> = Vec::new();
        let mut update_result_graphs: Vec<(String, PathBuf)> = Vec::new();

        if let Some(action) = &action_node {
            if let Some(action_props) = by_subj.get(action) {
                for (p, o) in action_props {
                    match p.as_str() {
                        s if s == QT_QUERY || s == UT_REQUEST => {
                            query_file = file_iri_to_path(o);
                        }
                        s if s == QT_DATA || s == UT_DATA => {
                            if let Some(p) = file_iri_to_path(o) {
                                data_files.push(p);
                            }
                        }
                        s if s == QT_GRAPH_DATA || s == UT_GRAPH_DATA => {
                            // Named graph node.
                            if let Some(gprops) = by_subj.get(o) {
                                let graph_iri = gprops
                                    .iter()
                                    .find(|(pp, _)| pp == UT_GRAPH_IRI)
                                    .map(|(_, v)| strip_literal(v))
                                    .unwrap_or_else(|| o.clone());
                                let file = gprops
                                    .iter()
                                    .find(|(pp, _)| pp == UT_GRAPH)
                                    .and_then(|(_, v)| file_iri_to_path(v));
                                if let Some(f) = file {
                                    named_graphs.push((graph_iri, f));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // For UPDATE tests, result may be a blank node with ut:data / ut:graphData.
        let result_file = if let Some(ref r) = result_node {
            if let Some(rprops) = by_subj.get(r.as_str()) {
                for (p, o) in rprops {
                    match p.as_str() {
                        s if s == UT_DATA => {
                            if let Some(f) = file_iri_to_path(o) {
                                update_result_data.push(f);
                            }
                        }
                        s if s == UT_GRAPH_DATA => {
                            if let Some(gprops) = by_subj.get(o) {
                                let graph_iri = gprops
                                    .iter()
                                    .find(|(pp, _)| pp == UT_GRAPH_IRI)
                                    .map(|(_, v)| strip_literal(v))
                                    .unwrap_or_else(|| o.clone());
                                let file = gprops
                                    .iter()
                                    .find(|(pp, _)| pp == UT_GRAPH)
                                    .and_then(|(_, v)| file_iri_to_path(v));
                                if let Some(f) = file {
                                    update_result_graphs.push((graph_iri, f));
                                }
                            }
                        }
                        _ => {}
                    }
                }
                None // Result is a blank node, not a file.
            } else {
                file_iri_to_path(r)
            }
        } else {
            None
        };

        cases.push(JenaTestCase {
            iri,
            name,
            test_type,
            category: category.to_string(),
            query_file,
            data_files,
            named_graphs,
            result_file,
            update_result_data,
            update_result_graphs,
        });
    }

    Ok(cases)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn subject_to_string(s: Subject<'_>) -> String {
    match s {
        Subject::NamedNode(n) => n.iri.to_string(),
        Subject::BlankNode(b) => format!("_:{}", b.id),
        Subject::Triple(_) => "_triple_".to_string(),
    }
}

fn term_to_string(t: Term<'_>) -> String {
    match t {
        Term::NamedNode(n) => n.iri.to_string(),
        Term::BlankNode(b) => format!("_:{}", b.id),
        Term::Literal(l) => match l {
            Literal::Simple { value } => value.to_string(),
            Literal::LanguageTaggedString { value, .. } => value.to_string(),
            Literal::Typed { value, .. } => value.to_string(),
        },
        Term::Triple(_) => "_triple_".to_string(),
    }
}

fn collect_list(head: &str, by_subj: &HashMap<String, Vec<(String, String)>>) -> Vec<String> {
    let mut items = Vec::new();
    let mut cur = head.to_string();
    while cur != RDF_NIL {
        let props = match by_subj.get(&cur) {
            Some(p) => p,
            None => break,
        };
        if let Some((_, first)) = props.iter().find(|(p, _)| p == RDF_FIRST) {
            items.push(first.clone());
        }
        match props.iter().find(|(p, _)| p == RDF_REST) {
            Some((_, rest)) => cur = rest.clone(),
            None => break,
        }
    }
    items
}

fn strip_literal(s: &str) -> String {
    // Remove `"..."` wrapping and language/datatype suffixes.
    let s = s.trim();
    if s.starts_with('"') {
        let s = &s[1..];
        if let Some(end) = s.rfind('"') {
            return s[..end].to_string();
        }
    }
    s.to_string()
}

fn file_iri_to_path(iri: &str) -> Option<PathBuf> {
    iri.strip_prefix("file://").map(PathBuf::from)
}
