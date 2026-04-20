//! W3C SPARQL 1.1 test manifest parser.
//!
//! Parses Turtle-format test manifests conforming to the W3C test manifest
//! vocabulary (`mf:`) and query-test vocabulary (`qt:`, `ut:`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rio_api::model::{Literal, Subject, Term};
use rio_api::parser::TriplesParser;
use rio_turtle::{TurtleError, TurtleParser};

// ── Well-known IRIs ───────────────────────────────────────────────────────────

const MF_MANIFEST: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#Manifest";
const MF_ENTRIES: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#entries";
const MF_NAME: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#name";
const MF_ACTION: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#action";
const MF_RESULT: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#result";
const MF_QUERY_EVAL: &str =
    "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#QueryEvaluationTest";
const MF_POS_SYNTAX: &str =
    "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#PositiveSyntaxTest11";
const MF_NEG_SYNTAX: &str =
    "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#NegativeSyntaxTest11";
const MF_UPDATE_EVAL: &str =
    "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#UpdateEvaluationTest";
const UT_UPDATE_EVAL: &str = "http://www.w3.org/2009/sparql/tests/test-update#UpdateEvaluationTest";
const QT_QUERY: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-query#query";
const QT_DATA: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-query#data";
const QT_GRAPH_DATA: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-query#graphData";
const QT_SERVICE_DATA: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-query#serviceData";
const QT_ENDPOINT: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-query#endpoint";
const QT_DATA_PROP: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-query#data";
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

/// A single W3C SPARQL 1.1 test case parsed from a manifest.
#[derive(Debug, Clone)]
pub struct TestCase {
    /// The test IRI (subject node in the manifest graph).
    pub iri: String,
    /// Human-readable test name (from `mf:name`).
    pub name: String,
    /// The type of this test.
    pub test_type: TestType,
    /// Sub-suite category name (e.g., `"aggregates"`, `"optional"`).
    pub category: String,
    /// Query or update file path (`None` for pure syntax tests with no action query).
    pub query_file: Option<PathBuf>,
    /// Default-graph data files (from `qt:data` / `ut:data`).
    pub data_files: Vec<PathBuf>,
    /// Named-graph files: `(graph IRI, file path)`.
    pub named_graphs: Vec<(String, PathBuf)>,
    /// Expected result file (`.srx`, `.srj`, or `.ttl`).
    pub result_file: Option<PathBuf>,
    /// SERVICE mock data: `(endpoint URL, data file path)`.
    /// Each entry represents a `qt:serviceData` block in the manifest.
    /// The endpoint data should be loaded into a named graph whose IRI is
    /// the endpoint URL before running the query.
    pub service_data: Vec<(String, PathBuf)>,
}

/// The kind of a W3C SPARQL test case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestType {
    /// `mf:QueryEvaluationTest` — run a query and compare results.
    QueryEvaluation,
    /// `ut:UpdateEvaluationTest` — run an UPDATE and compare the resulting graph.
    UpdateEvaluation,
    /// `mf:PositiveSyntaxTest11` — query must parse without error.
    PositiveSyntax,
    /// `mf:NegativeSyntaxTest11` — query must fail to parse.
    NegativeSyntax,
    /// Unrecognised type — skip these tests.
    NotClassified,
}

/// Parse all test cases from a W3C SPARQL 1.1 manifest Turtle file.
///
/// `category` is used as the `TestCase::category` field (e.g., `"aggregates"`).
/// Returns test cases in the order they appear in the manifest's `mf:entries` list.
pub fn parse_manifest(
    manifest_path: &Path,
    category: &str,
) -> Result<Vec<TestCase>, Box<dyn std::error::Error + Send + Sync>> {
    let manifest_dir = manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    // Canonicalize for a stable base IRI.
    let abs = manifest_path
        .canonicalize()
        .unwrap_or_else(|_| manifest_path.to_path_buf());
    let base_iri = format!("file://{}", abs.display());

    // Prepend an @base directive so relative IRIs are resolved against the
    // manifest's directory.  This avoids fighting with rio_turtle's lifetime
    // requirements for the base-IRI parameter.
    let raw = std::fs::read_to_string(manifest_path)
        .map_err(|e| format!("reading {}: {e}", manifest_path.display()))?;
    let content = format!("@base <{base_iri}> .\n{raw}");

    // Collect all triples into an in-memory adjacency map.
    // Keys: subject string ("file://..." or "_:bnode")
    // Values: HashMap<predicate, Vec<object>>
    let mut graph: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new();

    {
        let mut parser = TurtleParser::new(content.as_bytes(), None);
        parser.parse_all(&mut |t| -> Result<(), TurtleError> {
            let s = subject_to_string(&t.subject);
            let p = t.predicate.iri.to_string();
            let o = term_to_string(&t.object);
            graph.entry(s).or_default().entry(p).or_default().push(o);
            Ok(())
        })?;
    }

    // Find the manifest node (the subject typed as `mf:Manifest`).
    let manifest_node = graph
        .iter()
        .find(|(_, props)| {
            props
                .get(RDF_TYPE)
                .map(|types| types.iter().any(|t| t == MF_MANIFEST))
                .unwrap_or(false)
        })
        .map(|(k, _)| k.clone());

    // Get the head of the `mf:entries` RDF list.
    let entry_list_head = manifest_node
        .as_ref()
        .and_then(|m| graph.get(m))
        .and_then(|props| props.get(MF_ENTRIES))
        .and_then(|v| v.first())
        .cloned();

    // Walk the RDF list (rdf:first / rdf:rest) to collect test IRIs in order.
    let mut test_iris: Vec<String> = Vec::new();
    if let Some(head) = entry_list_head {
        let mut current = head;
        loop {
            if current == RDF_NIL {
                break;
            }
            let node_props = match graph.get(&current) {
                Some(p) => p,
                None => break,
            };
            if let Some(first_vals) = node_props.get(RDF_FIRST) {
                if let Some(first) = first_vals.first() {
                    test_iris.push(first.clone());
                }
            }
            match node_props.get(RDF_REST).and_then(|v| v.first()) {
                Some(rest) => current = rest.clone(),
                None => break,
            }
        }
    }

    // Parse each test case node.
    let mut test_cases = Vec::with_capacity(test_iris.len());
    for test_iri in test_iris {
        let props = match graph.get(&test_iri) {
            Some(p) => p,
            None => continue,
        };

        let name = props
            .get(MF_NAME)
            .and_then(|v| v.first())
            .map(|s| strip_literal(s))
            .unwrap_or_else(|| test_iri.clone());

        let test_type = detect_test_type(props.get(RDF_TYPE).map(Vec::as_slice).unwrap_or(&[]));

        let result_file = props
            .get(MF_RESULT)
            .and_then(|v| v.first())
            .and_then(|p| iri_to_path(p, &manifest_dir));

        let action_node = props.get(MF_ACTION).and_then(|v| v.first()).cloned();

        let mut query_file = None;
        let mut data_files: Vec<PathBuf> = Vec::new();
        let mut named_graphs: Vec<(String, PathBuf)> = Vec::new();
        let mut service_data: Vec<(String, PathBuf)> = Vec::new();

        if let Some(action) = action_node {
            if let Some(ap) = graph.get(&action) {
                // Query / update file
                for prop in [QT_QUERY, UT_REQUEST] {
                    if query_file.is_none() {
                        if let Some(q) = ap.get(prop).and_then(|v| v.first()) {
                            query_file = iri_to_path(q, &manifest_dir);
                        }
                    }
                }
            } else {
                // Syntax tests use `mf:action <query-file.rq>` directly — the
                // action IRI is the query file itself, not a blank node with
                // nested `qt:query`.
                query_file = iri_to_path(&action, &manifest_dir);
            }
            if let Some(ap) = graph.get(&action) {

                // Default graph data files
                for prop in [QT_DATA, UT_DATA] {
                    for d in ap.get(prop).unwrap_or(&vec![]) {
                        if let Some(p) = iri_to_path(d, &manifest_dir) {
                            data_files.push(p);
                        }
                    }
                }

                // Named graph files (qt:graphData)
                for ng_ref in ap.get(QT_GRAPH_DATA).unwrap_or(&vec![]) {
                    // graphData is typically a file IRI used directly as the graph IRI.
                    if let Some(ng_path) = iri_to_path(ng_ref, &manifest_dir) {
                        named_graphs.push((ng_ref.clone(), ng_path));
                    } else if let Some(ng_props) = graph.get(ng_ref) {
                        // Blank node with nested graph + graphIRI properties
                        let ng_iri = ng_props
                            .get(UT_GRAPH_IRI)
                            .or_else(|| ng_props.get(UT_GRAPH))
                            .and_then(|v| v.first())
                            .cloned()
                            .unwrap_or_else(|| ng_ref.clone());
                        let ng_path = ng_props
                            .get(UT_GRAPH)
                            .and_then(|v| v.first())
                            .and_then(|p| iri_to_path(p, &manifest_dir));
                        if let Some(path) = ng_path {
                            named_graphs.push((ng_iri, path));
                        }
                    }
                }

                // Named graph files (ut:graphData for update tests)
                for ng_ref in ap.get(UT_GRAPH_DATA).unwrap_or(&vec![]) {
                    if let Some(ng_props) = graph.get(ng_ref) {
                        let ng_iri = ng_props
                            .get(UT_GRAPH_IRI)
                            .and_then(|v| v.first())
                            .cloned()
                            .unwrap_or_else(|| ng_ref.clone());
                        let ng_path = ng_props
                            .get(UT_GRAPH)
                            .or_else(|| ng_props.get(UT_DATA))
                            .and_then(|v| v.first())
                            .and_then(|p| iri_to_path(p, &manifest_dir));
                        if let Some(path) = ng_path {
                            named_graphs.push((ng_iri, path));
                        }
                    }
                }

                // SERVICE mock data (qt:serviceData), v0.42.0.
                // Each qt:serviceData blank node has qt:endpoint (the URL) and
                // qt:data (the RDF file to load into that named graph).
                for sd_ref in ap.get(QT_SERVICE_DATA).unwrap_or(&vec![]) {
                    if let Some(sd_props) = graph.get(sd_ref) {
                        let endpoint_url = sd_props
                            .get(QT_ENDPOINT)
                            .and_then(|v| v.first())
                            .cloned();
                        let data_file = sd_props
                            .get(QT_DATA_PROP)
                            .and_then(|v| v.first())
                            .and_then(|p| iri_to_path(p, &manifest_dir));
                        if let (Some(url), Some(path)) = (endpoint_url, data_file) {
                            service_data.push((url, path));
                        }
                    }
                }
            }
        }

        test_cases.push(TestCase {
            iri: test_iri,
            name,
            test_type,
            category: category.to_string(),
            query_file,
            data_files,
            named_graphs,
            result_file,
            service_data,
        });
    }

    Ok(test_cases)
}

// ── Private helpers ────────────────────────────────────────────────────────────

fn detect_test_type(types: &[String]) -> TestType {
    for t in types {
        match t.as_str() {
            MF_QUERY_EVAL => return TestType::QueryEvaluation,
            UT_UPDATE_EVAL | MF_UPDATE_EVAL => return TestType::UpdateEvaluation,
            MF_POS_SYNTAX => return TestType::PositiveSyntax,
            MF_NEG_SYNTAX => return TestType::NegativeSyntax,
            _ => {}
        }
    }
    TestType::NotClassified
}

fn subject_to_string(s: &Subject<'_>) -> String {
    match s {
        Subject::NamedNode(n) => n.iri.to_string(),
        Subject::BlankNode(b) => format!("_:{}", b.id),
        Subject::Triple(_) => "_:quoted".to_string(),
    }
}

fn term_to_string(t: &Term<'_>) -> String {
    match t {
        Term::NamedNode(n) => n.iri.to_string(),
        Term::BlankNode(b) => format!("_:{}", b.id),
        Term::Literal(l) => match l {
            Literal::Simple { value } => value.to_string(),
            Literal::LanguageTaggedString { value, .. } => value.to_string(),
            Literal::Typed { value, .. } => value.to_string(),
        },
        Term::Triple(_) => "_:quoted".to_string(),
    }
}

/// Convert a `file://` IRI to a local path; fall back to treating the string
/// as a path relative to `base_dir`.
fn iri_to_path(iri: &str, base_dir: &Path) -> Option<PathBuf> {
    if let Some(path_str) = iri.strip_prefix("file://") {
        let p = PathBuf::from(path_str);
        if p.exists() {
            return Some(p);
        }
        return None;
    }
    // Try as a path relative to the manifest directory (for non-file:// IRIs
    // that are actually relative paths left unresolved by the parser).
    if !iri.contains("://") && !iri.starts_with("urn:") {
        let p = base_dir.join(iri);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// Strip literal quoting and datatype/language annotations.
fn strip_literal(s: &str) -> String {
    // rio_turtle already strips the quotes from Simple literals, returning just the value.
    s.to_string()
}
