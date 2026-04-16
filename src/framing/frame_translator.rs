//! Frame translator — JSON-LD frame → SPARQL CONSTRUCT query string.
//!
//! Implements the mapping described in §4.12.1 of the implementation plan.
//! All IRI values are treated as full IRI strings (not yet dictionary-encoded
//! here; the SPARQL engine handles encoding during SQL generation).

use serde_json::Value;

/// Translate a JSON-LD frame document to a SPARQL CONSTRUCT query string.
///
/// Returns an error string on invalid input (PT710/PT712).
pub fn translate(frame: &Value, graph_iri: Option<&str>) -> Result<String, String> {
    let obj = match frame.as_object() {
        Some(o) => o,
        None => return Err("PT710: frame must be a JSON object".to_owned()),
    };

    let max_depth = crate::MAX_PATH_DEPTH.get() as usize;
    let mut ctx = TranslateCtx {
        depth: 0,
        max_depth,
        var_counter: 0,
        template_lines: Vec::new(),
        where_clauses: Vec::new(),
    };

    // Root subject variable.
    let root_var = "?_root".to_owned();

    ctx.translate_frame_object(obj, &root_var)?;

    // Build CONSTRUCT template.
    let template = ctx.template_lines.join("\n    ");

    // Build WHERE clause.
    let where_body = ctx.where_clauses.join("\n    ");

    // Apply named-graph filter if requested.
    let graph_filter = match graph_iri {
        Some(g) => {
            let g_clean = strip_angle_brackets(g);
            format!("\n    FILTER(?_g = <{g_clean}>)")
        }
        None => String::new(),
    };

    let sparql =
        format!("CONSTRUCT {{\n    {template}\n}} WHERE {{\n    {where_body}{graph_filter}\n}}");

    Ok(sparql)
}

// ─── Internal context ─────────────────────────────────────────────────────────

struct TranslateCtx {
    depth: usize,
    max_depth: usize,
    var_counter: usize,
    template_lines: Vec<String>,
    where_clauses: Vec<String>,
}

impl TranslateCtx {
    fn fresh_var(&mut self) -> String {
        let v = format!("?_v{}_{}", self.depth, self.var_counter);
        self.var_counter += 1;
        v
    }

    /// Translate a frame object (at any nesting level) for subject variable `subj`.
    fn translate_frame_object(
        &mut self,
        obj: &serde_json::Map<String, Value>,
        subj: &str,
    ) -> Result<(), String> {
        if self.depth >= self.max_depth {
            return Err(format!(
                "PT712: frame nesting depth {} exceeds pg_ripple.max_path_depth ({})",
                self.depth, self.max_depth
            ));
        }

        let require_all = obj
            .get("@requireAll")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Handle @type constraint.
        if let Some(type_val) = obj.get("@type") {
            self.translate_type_constraint(subj, type_val)?;
        }

        // Handle @id matching (FILTER).
        if let Some(id_val) = obj.get("@id") {
            match id_val {
                Value::String(iri) => {
                    let clean = strip_angle_brackets(iri);
                    self.where_clauses
                        .push(format!("FILTER({subj} = <{clean}>)"));
                }
                Value::Array(iris) => {
                    let ids: Vec<String> = iris
                        .iter()
                        .filter_map(Value::as_str)
                        .map(|s| format!("<{}>", strip_angle_brackets(s)))
                        .collect();
                    if !ids.is_empty() {
                        let list = ids.join(", ");
                        self.where_clauses
                            .push(format!("FILTER({subj} IN ({list}))"));
                    }
                }
                _ => {}
            }
        }

        // Handle @reverse.
        if let Some(rev_val) = obj.get("@reverse")
            && let Some(rev_obj) = rev_val.as_object()
        {
            for (pred_iri, child_frame) in rev_obj {
                let clean_pred = strip_angle_brackets(pred_iri);
                let rev_var = self.fresh_var();
                // ?rev_var <pred> subj (flipped direction)
                let triple = format!("{rev_var} <{clean_pred}> {subj}");
                self.template_lines.push(format!("{triple} ."));
                if require_all {
                    self.where_clauses.push(format!("{triple} ."));
                } else {
                    self.where_clauses.push(format!("OPTIONAL {{ {triple} ."));
                }
                if let Some(child_obj) = child_frame.as_object() {
                    self.depth += 1;
                    self.translate_frame_object(child_obj, &rev_var)?;
                    self.depth -= 1;
                }
                if !require_all {
                    self.where_clauses.push("}".to_owned());
                }
            }
        }

        // Handle property entries.
        for (key, val) in obj {
            if key.starts_with('@') {
                continue;
            }

            let clean_pred = strip_angle_brackets(key);
            self.translate_property(subj, clean_pred, val, require_all)?;
        }

        Ok(())
    }

    fn translate_type_constraint(&mut self, subj: &str, type_val: &Value) -> Result<(), String> {
        match type_val {
            Value::String(iri) => {
                let clean = strip_angle_brackets(iri);
                let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
                let triple = format!("{subj} <{rdf_type}> <{clean}>");
                self.template_lines.push(format!("{triple} ."));
                self.where_clauses.push(format!("{triple} ."));
            }
            Value::Array(types) => {
                let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
                let type_var = self.fresh_var();
                let iris: Vec<String> = types
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|s| format!("<{}>", strip_angle_brackets(s)))
                    .collect();
                let triple = format!("{subj} <{rdf_type}> {type_var}");
                self.template_lines.push(format!("{triple} ."));
                self.where_clauses.push(format!("{triple} ."));
                if !iris.is_empty() {
                    let list = iris.join(", ");
                    self.where_clauses
                        .push(format!("FILTER({type_var} IN ({list}))"));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn translate_property(
        &mut self,
        subj: &str,
        pred_iri: &str,
        val: &Value,
        require_all: bool,
    ) -> Result<(), String> {
        match val {
            // Property wildcard `{}` — OPTIONAL { ?s <p> ?v }
            Value::Object(child_obj) if child_obj.is_empty() => {
                let obj_var = self.fresh_var();
                let triple = format!("{subj} <{pred_iri}> {obj_var}");
                self.template_lines.push(format!("{triple} ."));
                if require_all {
                    self.where_clauses.push(format!("{triple} ."));
                } else {
                    self.where_clauses
                        .push(format!("OPTIONAL {{ {triple} . }}"));
                }
            }
            // Nested frame — OPTIONAL { ?s <p> ?n . <nested patterns for ?n> }
            Value::Object(child_obj) => {
                let node_var = self.fresh_var();
                let triple = format!("{subj} <{pred_iri}> {node_var}");
                self.template_lines.push(format!("{triple} ."));
                if require_all {
                    self.where_clauses.push(format!("{triple} ."));
                    self.depth += 1;
                    self.translate_frame_object(child_obj, &node_var)?;
                    self.depth -= 1;
                } else {
                    self.where_clauses.push(format!("OPTIONAL {{ {triple} ."));
                    self.depth += 1;
                    self.translate_frame_object(child_obj, &node_var)?;
                    self.depth -= 1;
                    self.where_clauses.push("}".to_owned());
                }
            }
            // Absent-property pattern `[]` — OPTIONAL { ?s <p> ?absent } FILTER(!bound(?absent))
            Value::Array(arr) if arr.is_empty() => {
                let absent_var = self.fresh_var();
                let triple = format!("{subj} <{pred_iri}> {absent_var}");
                self.where_clauses
                    .push(format!("OPTIONAL {{ {triple} . }}"));
                self.where_clauses
                    .push(format!("FILTER(!bound({absent_var}))"));
            }
            _ => {
                // Any other array or scalar — treat as wildcard.
                let obj_var = self.fresh_var();
                let triple = format!("{subj} <{pred_iri}> {obj_var}");
                self.template_lines.push(format!("{triple} ."));
                if require_all {
                    self.where_clauses.push(format!("{triple} ."));
                } else {
                    self.where_clauses
                        .push(format!("OPTIONAL {{ {triple} . }}"));
                }
            }
        }
        Ok(())
    }
}

// ─── Utility ──────────────────────────────────────────────────────────────────

/// Strip surrounding `<` `>` angle brackets from an IRI string if present.
pub(crate) fn strip_angle_brackets(s: &str) -> &str {
    if s.starts_with('<') && s.ends_with('>') {
        &s[1..s.len() - 1]
    } else {
        s
    }
}
