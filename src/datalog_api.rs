//! pg_ripple SQL API — Datalog Reasoning Engine (v0.10.0+)

#[pgrx::pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    // ── Datalog Reasoning Engine (v0.10.0) ────────────────────────────────────

    /// Load Datalog rules from a text string.
    ///
    /// `rules` is a Turtle-flavoured Datalog rule set.
    /// `rule_set` is the name for this group of rules (default: 'custom').
    /// Returns the number of rules stored.
    #[pg_extern]
    fn load_rules(rules: &str, rule_set: default!(&str, "'custom'")) -> i64 {
        crate::datalog::builtins::register_standard_prefixes();
        // Invalidate plan cache for this rule set (v0.30.0).
        crate::datalog::cache::invalidate(rule_set);
        // Invalidate tabling cache (v0.32.0).
        crate::datalog::tabling_invalidate_all();
        let rule_set_ir = match crate::datalog::parse_rules(rules, rule_set) {
            Ok(rs) => rs,
            Err(e) => pgrx::error!("rule parse error: {e}"),
        };
        crate::datalog::store_rules(rule_set, &rule_set_ir.rules)
    }

    /// Load a built-in rule set by name.
    ///
    /// Supported names: `'rdfs'`, `'owl-rl'`.
    /// Returns the number of rules stored.
    #[pg_extern]
    fn load_rules_builtin(name: &str) -> i64 {
        crate::datalog::builtins::register_standard_prefixes();
        let text = match crate::datalog::builtins::get_builtin_rules(name) {
            Ok(t) => t,
            Err(e) => pgrx::error!("{e}"),
        };
        let rule_set_ir = match crate::datalog::parse_rules(text, name) {
            Ok(rs) => rs,
            Err(e) => pgrx::error!("built-in rule parse error: {e}"),
        };
        crate::datalog::store_rules(name, &rule_set_ir.rules)
    }

    /// List all stored Datalog rules as JSONB rows.
    ///
    /// Returns one row per rule with fields: id, rule_set, rule_text, head_pred,
    /// stratum, is_recursive, active.
    #[pg_extern]
    fn list_rules() -> pgrx::JsonB {
        crate::datalog::ensure_catalog();
        let rows = pgrx::Spi::connect(|client| {
            client
                .select(
                    "SELECT id, rule_set, rule_text, head_pred, stratum, is_recursive, active \
                     FROM _pg_ripple.rules \
                     ORDER BY rule_set, stratum, id",
                    None,
                    &[],
                )
                .unwrap_or_else(|e| pgrx::error!("list_rules SPI error: {e}"))
                .map(|row| {
                    let mut obj = serde_json::Map::new();
                    obj.insert(
                        "id".to_owned(),
                        row.get::<i64>(1)
                            .ok()
                            .flatten()
                            .map(serde_json::Value::from)
                            .unwrap_or(serde_json::Value::Null),
                    );
                    obj.insert(
                        "rule_set".to_owned(),
                        row.get::<String>(2)
                            .ok()
                            .flatten()
                            .map(serde_json::Value::String)
                            .unwrap_or(serde_json::Value::Null),
                    );
                    obj.insert(
                        "rule_text".to_owned(),
                        row.get::<String>(3)
                            .ok()
                            .flatten()
                            .map(serde_json::Value::String)
                            .unwrap_or(serde_json::Value::Null),
                    );
                    obj.insert(
                        "stratum".to_owned(),
                        row.get::<i32>(5)
                            .ok()
                            .flatten()
                            .map(|v| serde_json::Value::from(v as i64))
                            .unwrap_or(serde_json::Value::Null),
                    );
                    obj.insert(
                        "is_recursive".to_owned(),
                        row.get::<bool>(6)
                            .ok()
                            .flatten()
                            .map(serde_json::Value::Bool)
                            .unwrap_or(serde_json::Value::Null),
                    );
                    obj.insert(
                        "active".to_owned(),
                        row.get::<bool>(7)
                            .ok()
                            .flatten()
                            .map(serde_json::Value::Bool)
                            .unwrap_or(serde_json::Value::Null),
                    );
                    serde_json::Value::Object(obj)
                })
                .collect::<Vec<_>>()
        });
        pgrx::JsonB(serde_json::Value::Array(rows))
    }

    /// Drop all rules in the named rule set.
    ///
    /// Returns the number of rules deleted.
    #[pg_extern]
    fn drop_rules(rule_set: &str) -> i64 {
        crate::datalog::ensure_catalog();
        // Invalidate plan cache for this rule set (v0.30.0).
        crate::datalog::cache::invalidate(rule_set);
        // Invalidate tabling cache (v0.32.0).
        crate::datalog::tabling_invalidate_all();
        pgrx::Spi::get_one_with_args::<i64>(
            "WITH deleted AS ( \
                 DELETE FROM _pg_ripple.rules WHERE rule_set = $1 RETURNING 1 \
             ) SELECT count(*) FROM deleted",
            &[pgrx::datum::DatumWithOid::from(rule_set)],
        )
        .unwrap_or(None)
        .unwrap_or(0)
    }

    // ── v0.34.0: Incremental rule updates ────────────────────────────────────

    /// Add a single rule to an existing rule set (v0.34.0).
    ///
    /// The rule is parsed, stored in the catalog, and its head predicate gets
    /// one fresh seed pass against the current VP tables.  Other derived
    /// predicates are not affected.  Returns the new rule's catalog ID.
    ///
    /// This is more efficient than calling `drop_rules()` + `load_rules()` when
    /// adding rules to a large live rule set, because only the new rule's derived
    /// predicate needs re-evaluation.
    #[pg_extern]
    fn add_rule(rule_set: &str, rule_text: &str) -> i64 {
        match crate::datalog::add_rule_to_set(rule_set, rule_text) {
            Ok(id) => id,
            Err(e) => pgrx::error!("add_rule error: {e}"),
        }
    }

    /// Remove a single rule by its catalog ID (v0.34.0).
    ///
    /// The rule is marked inactive and any derived facts solely supported by it
    /// are retracted using DRed (when `pg_ripple.dred_enabled = true`).  Falls
    /// back to full re-materialization when DRed is disabled or detects a cycle
    /// (error code PT530).  Returns the number of derived triples permanently
    /// retracted.
    ///
    /// Obtain the rule ID from `pg_ripple.list_rules()`.
    #[pg_extern]
    fn remove_rule(rule_id: i64) -> i64 {
        match crate::datalog::remove_rule_by_id(rule_id) {
            Ok(n) => n,
            Err(e) => pgrx::error!("remove_rule error: {e}"),
        }
    }

    /// Invoke DRed incremental retraction for a deleted base triple (v0.34.0).
    ///
    /// Normally called automatically by the CDC delete path.  This function
    /// exposes the DRed algorithm for testing and manual invocation.
    ///
    /// `pred_id` — dictionary ID of the deleted triple's predicate.
    /// `s_val`   — dictionary ID of the deleted triple's subject.
    /// `o_val`   — dictionary ID of the deleted triple's object.
    /// `g_val`   — dictionary ID of the deleted triple's graph (0 = default).
    ///
    /// Returns the number of derived triples permanently retracted.
    #[pg_extern]
    fn dred_on_delete(pred_id: i64, s_val: i64, o_val: i64, g_val: i64) -> i64 {
        crate::datalog::run_dred_on_delete(pred_id, s_val, o_val, g_val)
    }

    /// Enable a named rule set (set active = true).
    #[pg_extern]
    fn enable_rule_set(name: &str) {
        crate::datalog::ensure_catalog();
        let _ = pgrx::Spi::run_with_args(
            "UPDATE _pg_ripple.rules SET active = true WHERE rule_set = $1; \
             UPDATE _pg_ripple.rule_sets SET active = true WHERE name = $1",
            &[pgrx::datum::DatumWithOid::from(name)],
        );
    }

    /// Disable a named rule set (set active = false) without dropping it.
    #[pg_extern]
    fn disable_rule_set(name: &str) {
        crate::datalog::ensure_catalog();
        let _ = pgrx::Spi::run_with_args(
            "UPDATE _pg_ripple.rules SET active = false WHERE rule_set = $1; \
             UPDATE _pg_ripple.rule_sets SET active = false WHERE name = $1",
            &[pgrx::datum::DatumWithOid::from(name)],
        );
    }

    /// Run inference for the named rule set and materialise derived triples.
    ///
    /// Returns the number of triples derived.
    #[pg_extern]
    fn infer(rule_set: default!(&str, "'custom'")) -> i64 {
        crate::datalog::run_inference(rule_set)
    }

    /// Run semi-naive inference for the named rule set and materialise derived triples.
    ///
    /// Returns a JSONB object with:
    /// - `"derived"`: total number of triples derived (i64)
    /// - `"iterations"`: number of fixpoint iterations performed (i32)
    /// - `"eliminated_rules"`: array of rule texts eliminated by subsumption checking (v0.29.0)
    /// - `"parallel_groups"`: number of independent rule groups detected in the first stratum (v0.35.0)
    /// - `"max_concurrent"`: effective worker count that would be used given `datalog_parallel_workers` (v0.35.0)
    ///
    /// Semi-naive evaluation avoids re-examining unchanged rows on each iteration,
    /// achieving iteration counts bounded by the longest derivation chain rather
    /// than the full relation size.  Subsumption checking (v0.29.0) removes rules
    /// whose body is a superset of another rule's body, reducing SQL statements per
    /// iteration.
    #[pg_extern]
    fn infer_with_stats(rule_set: default!(&str, "'custom'")) -> pgrx::JsonB {
        let (derived, iterations, eliminated, parallel_groups, max_concurrent) =
            crate::datalog::run_inference_seminaive_full(rule_set);
        let mut obj = serde_json::Map::new();
        obj.insert(
            "derived".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(derived)),
        );
        obj.insert(
            "iterations".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(iterations)),
        );
        obj.insert(
            "eliminated_rules".to_owned(),
            serde_json::Value::Array(
                eliminated
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
        obj.insert(
            "parallel_groups".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(parallel_groups as i64)),
        );
        obj.insert(
            "max_concurrent".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(max_concurrent as i64)),
        );
        pgrx::JsonB(serde_json::Value::Object(obj))
    }

    /// Run goal-directed inference using magic sets (v0.29.0).
    ///
    /// Materialises only facts relevant to the goal triple pattern and returns
    /// a JSONB object with:
    /// - `"derived"`: total triples derived by inference
    /// - `"iterations"`: fixpoint iteration count
    /// - `"matching"`: count of triples in the store matching the goal pattern
    ///
    /// The `goal` parameter is a whitespace-delimited triple pattern:
    /// - `?varname` — free variable (any value matches)
    /// - `<iri>` — bound IRI
    /// - `prefix:local` — bound prefixed IRI
    /// - `"literal"` — bound literal
    ///
    /// Example: `pg_ripple.infer_goal('rdfs', '?x rdf:type foaf:Person')`
    ///
    /// When `pg_ripple.magic_sets = false`, runs full materialization and
    /// filters the results post-hoc (functionally correct but slower).
    #[pg_extern]
    fn infer_goal(rule_set: &str, goal: &str) -> pgrx::JsonB {
        let goal_pattern = match crate::datalog::parse_goal(goal) {
            Ok(g) => g,
            Err(e) => {
                pgrx::warning!("infer_goal: failed to parse goal '{}': {e}", goal);
                // Return empty result on parse error.
                let mut obj = serde_json::Map::new();
                obj.insert(
                    "derived".to_owned(),
                    serde_json::Value::Number(serde_json::Number::from(0i64)),
                );
                obj.insert(
                    "iterations".to_owned(),
                    serde_json::Value::Number(serde_json::Number::from(0i32)),
                );
                obj.insert(
                    "matching".to_owned(),
                    serde_json::Value::Number(serde_json::Number::from(0i64)),
                );
                return pgrx::JsonB(serde_json::Value::Object(obj));
            }
        };

        let (matching, derived, iterations) =
            match crate::datalog::run_infer_goal(rule_set, &goal_pattern) {
                Ok(r) => r,
                Err(e) => {
                    pgrx::warning!("infer_goal: inference failed: {e}");
                    (0, 0, 0)
                }
            };

        let mut obj = serde_json::Map::new();
        obj.insert(
            "derived".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(derived)),
        );
        obj.insert(
            "iterations".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(iterations)),
        );
        obj.insert(
            "matching".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(matching)),
        );
        pgrx::JsonB(serde_json::Value::Object(obj))
    }

    /// Run inference for a rule set that may contain aggregate body literals
    /// (Datalog^agg, v0.30.0).
    ///
    /// Supports `COUNT(?aggVar WHERE subject pred object) = ?resultVar` syntax
    /// in rule bodies.  Aggregate rules derive facts by grouping over a base
    /// predicate and computing COUNT, SUM, MIN, MAX, or AVG per group.
    ///
    /// Returns a JSONB object with:
    /// - `"derived"`: total triples derived (aggregate + non-aggregate)
    /// - `"aggregate_derived"`: triples derived by aggregate rules only
    /// - `"iterations"`: fixpoint iteration count for non-aggregate rules
    ///
    /// Emits a WARNING with PT510 code if aggregation-stratification is violated.
    #[pg_extern]
    fn infer_agg(rule_set: default!(&str, "'custom'")) -> pgrx::JsonB {
        let (total, agg_derived, iterations) = crate::datalog::run_inference_agg(rule_set);
        let mut obj = serde_json::Map::new();
        obj.insert(
            "derived".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(total)),
        );
        obj.insert(
            "aggregate_derived".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(agg_derived)),
        );
        obj.insert(
            "iterations".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(iterations)),
        );
        pgrx::JsonB(serde_json::Value::Object(obj))
    }

    /// Run inference for a rule set, restricted to rules that can contribute to
    /// the given demand patterns (demand transformation, v0.31.0).
    ///
    /// `demands` is a JSONB array of goal patterns, e.g.:
    /// ```json
    /// [{"p": "<https://example.org/transitive>"}, {"s": "<https://ex.org/a>", "p": "<https://ex.org/childOf>"}]
    /// ```
    /// Each element has optional `"s"`, `"p"`, `"o"` keys with IRI values.
    /// Omitted keys are treated as free variables.
    ///
    /// When `demands` is an empty array (`'[]'`), runs full inference (same as
    /// `infer()`).
    ///
    /// Returns a JSONB object with:
    /// - `"derived"`: total triples derived
    /// - `"iterations"`: fixpoint iteration count
    /// - `"demand_predicates"`: array of predicate IRI strings that were used as
    ///   demand seeds (decoded from dictionary)
    ///
    /// Also applies `owl:sameAs` canonicalization when
    /// `pg_ripple.sameas_reasoning` is `on` (default).
    #[pg_extern]
    fn infer_demand(
        rule_set: default!(&str, "'custom'"),
        demands: default!(pgrx::JsonB, "'[]'::jsonb"),
    ) -> pgrx::JsonB {
        let demands_str = demands.0.to_string();
        let demand_specs = crate::datalog::parse_demands_json(&demands_str);

        let (derived, iterations, demand_pred_ids) =
            crate::datalog::run_infer_demand(rule_set, &demand_specs);

        // Decode demand predicate IDs back to IRI strings for the output.
        let demand_preds_json: serde_json::Value = if demand_pred_ids.is_empty() {
            serde_json::Value::Array(vec![])
        } else {
            let decoded: Vec<serde_json::Value> = demand_pred_ids
                .iter()
                .filter_map(|&id| crate::dictionary::decode(id).map(serde_json::Value::String))
                .collect();
            serde_json::Value::Array(decoded)
        };

        let mut obj = serde_json::Map::new();
        obj.insert(
            "derived".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(derived)),
        );
        obj.insert(
            "iterations".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(iterations)),
        );
        obj.insert("demand_predicates".to_owned(), demand_preds_json);
        pgrx::JsonB(serde_json::Value::Object(obj))
    }

    /// Return statistics for the Datalog rule plan cache (v0.30.0).
    ///
    /// Each row has:
    /// - `rule_set TEXT` — the rule set name
    /// - `hits BIGINT` — number of times the cached SQL was used
    /// - `misses BIGINT` — number of times the cache was consulted but missed
    /// - `entries INT` — total number of entries currently in the cache
    #[pg_extern]
    fn rule_plan_cache_stats() -> TableIterator<
        'static,
        (
            name!(rule_set, String),
            name!(hits, i64),
            name!(misses, i64),
            name!(entries, i32),
        ),
    > {
        let stats = crate::datalog::cache::stats();
        TableIterator::new(
            stats
                .into_iter()
                .map(|s| (s.rule_set, s.hits, s.misses, s.entries)),
        )
    }

    // ── v0.32.0: Well-Founded Semantics ───────────────────────────────────────

    /// Run well-founded semantics inference for the named rule set (v0.32.0).
    ///
    /// For **stratifiable programs** (no cyclic negation): identical to
    /// `infer_with_stats()` — all derived facts have `certainty = 'true'`.
    ///
    /// For **non-stratifiable programs** (cyclic negation detected):
    /// - Facts derivable from purely positive rules → `certainty = 'true'`
    ///   (materialised into VP tables like normal inference).
    /// - Facts only derivable via negation of uncertain atoms → `certainty = 'unknown'`
    ///   (reported in the JSONB output but NOT materialised into VP tables).
    ///
    /// Returns a JSONB object with:
    /// - `"derived"`: total facts (certain + unknown)
    /// - `"certain"`: facts with `certainty = 'true'`
    /// - `"unknown"`: facts with `certainty = 'unknown'`
    /// - `"iterations"`: number of fixpoint passes performed
    /// - `"stratifiable"`: `true` if the program is stratifiable, `false` otherwise
    ///
    /// GUC: `pg_ripple.wfs_max_iterations` (default 100) — safety cap per pass.
    /// Emits WARNING PT520 if a pass does not converge within the limit.
    #[pg_extern]
    fn infer_wfs(rule_set: default!(&str, "'custom'")) -> pgrx::JsonB {
        // Ensure the tabling catalog exists before any call that may try to
        // invalidate it (tabling_invalidate_all checks for the table first).
        crate::datalog::ensure_tabling_catalog();

        // Check tabling cache for a previous result.
        let goal_hash = crate::datalog::compute_goal_hash(&format!("wfs:{rule_set}"));
        if let Some(cached) = crate::datalog::tabling_lookup(goal_hash) {
            return pgrx::JsonB(cached);
        }

        // Cache miss — run WFS inference.
        let start = std::time::Instant::now();
        let (certain, unknown, total, iters, stratifiable) = crate::datalog::run_wfs(rule_set);
        let result = crate::datalog::build_wfs_jsonb(certain, unknown, total, iters, stratifiable);
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

        // Store result in the tabling cache for future calls.
        crate::datalog::tabling_store(goal_hash, &result.0, elapsed_ms);

        result
    }

    // ── v0.36.0: WCOJ & Lattice-Based Datalog ────────────────────────────────

    /// Detect whether a SPARQL triangle query is cyclic (v0.36.0).
    ///
    /// Returns `true` if the provided BGP variable pattern sets contain a cycle
    /// (i.e. the variable adjacency graph has a back-edge).  Used internally
    /// by the SPARQL→SQL translator; also exposed for testing and introspection.
    ///
    /// Each row in `pattern_vars_json` is a JSON array of variable name strings
    /// representing the variables co-occurring in one triple pattern.
    ///
    /// Example:
    /// ```sql
    /// SELECT pg_ripple.wcoj_is_cyclic('[["a","b"],["b","c"],["c","a"]]');
    /// -- returns true
    /// ```
    #[pg_extern]
    fn wcoj_is_cyclic(pattern_vars_json: &str) -> bool {
        let patterns: Vec<Vec<String>> = match serde_json::from_str(pattern_vars_json) {
            Ok(v) => v,
            Err(e) => pgrx::error!("wcoj_is_cyclic: invalid JSON input: {e}"),
        };
        crate::sparql::wcoj::detect_cyclic_bgp(&patterns)
    }

    /// Run a triangle-detection query on a VP predicate and return result stats (v0.36.0).
    ///
    /// Returns JSONB with `{"triangle_count": N, "wcoj_applied": bool, "predicate_iri": "..."}`.
    ///
    /// `predicate_iri` — the predicate IRI (without angle brackets) to use for all
    /// three edges of the triangle.
    ///
    /// This function is primarily used by `benchmarks/wcoj.sql` to compare
    /// WCOJ vs. standard planner execution.
    #[pg_extern]
    fn wcoj_triangle_query(predicate_iri: &str) -> pgrx::JsonB {
        let result = crate::sparql::wcoj::run_triangle_query(predicate_iri);
        pgrx::JsonB(serde_json::json!({
            "triangle_count": result.triangle_count,
            "wcoj_applied":   result.wcoj_applied,
            "predicate_iri":  result.predicate_iri
        }))
    }

    /// Register a user-defined lattice type for Datalog^L rules (v0.36.0).
    ///
    /// A lattice is an algebraic structure (L, ⊔) where the join operation ⊔
    /// is commutative, associative, and idempotent, with a bottom element ⊥.
    /// Fixpoint computation on a lattice terminates when the ascending chain
    /// condition holds.
    ///
    /// # Parameters
    ///
    /// - `name` — unique lattice identifier (e.g. `'trust'`, `'my_lattice'`).
    /// - `join_fn` — PostgreSQL aggregate function name implementing the join
    ///   (e.g. `'min'`, `'max'`, `'array_agg'`, `'my_custom_agg'`).
    ///   Must be commutative and associative.
    /// - `bottom` — bottom element as a text string (e.g. `'9223372036854775807'`
    ///   for a MinLattice over integer trust scores).
    ///
    /// Returns `true` if the lattice was newly registered, `false` if it already
    /// existed.
    ///
    /// # Built-in lattices
    ///
    /// The following lattices are pre-registered and do not need to be created:
    /// - `'min'` — MinLattice (join = MIN, bottom = i64::MAX)
    /// - `'max'` — MaxLattice (join = MAX, bottom = i64::MIN)
    /// - `'set'` — SetLattice (join = UNION via array_agg, bottom = {})
    /// - `'interval'` — IntervalLattice (join = MAX, bottom = 0)
    ///
    /// # Example
    ///
    /// ```sql
    /// -- Register a MinLattice for trust propagation over [0.0, 1.0] scores.
    /// SELECT pg_ripple.create_lattice('trust_score', 'min', '1.0');
    /// ```
    #[pg_extern]
    fn create_lattice(name: &str, join_fn: &str, bottom: &str) -> bool {
        crate::datalog::register_lattice(name, join_fn, bottom)
    }

    /// List all registered lattice types as JSONB (v0.36.0).
    ///
    /// Returns an array of `{"name": "...", "join_fn": "...", "bottom": "...", "builtin": bool}`.
    #[pg_extern]
    fn list_lattices() -> pgrx::JsonB {
        crate::datalog::ensure_lattice_catalog();
        let rows: Vec<serde_json::Value> = pgrx::Spi::connect(|c| {
            c.select(
                "SELECT name, join_fn, bottom, builtin \
                 FROM _pg_ripple.lattice_types \
                 ORDER BY builtin DESC, name",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("list_lattices: SPI error: {e}"))
            .map(|row| {
                let name: String = row.get::<String>(1).ok().flatten().unwrap_or_default();
                let join_fn: String = row.get::<String>(2).ok().flatten().unwrap_or_default();
                let bottom: String = row.get::<String>(3).ok().flatten().unwrap_or_default();
                let builtin: bool = row.get::<bool>(4).ok().flatten().unwrap_or(false);
                serde_json::json!({
                    "name":    name,
                    "join_fn": join_fn,
                    "bottom":  bottom,
                    "builtin": builtin
                })
            })
            .collect()
        });
        pgrx::JsonB(serde_json::Value::Array(rows))
    }

    /// Run lattice-based Datalog inference for a rule set (v0.36.0).
    ///
    /// Executes a monotone fixpoint computation over the rules in `rule_set`
    /// using `lattice_name` as the lattice type for head derivations.
    ///
    /// Terminates when no new values are derived (convergence), or when
    /// `pg_ripple.lattice_max_iterations` is reached (emits WARNING PT540 and
    /// returns partial results).
    ///
    /// Returns JSONB with:
    /// - `"derived"` — total new lattice values written
    /// - `"iterations"` — fixpoint iterations performed
    /// - `"lattice"` — name of the lattice used
    /// - `"rule_set"` — name of the rule set evaluated
    ///
    /// # Example
    ///
    /// ```sql
    /// -- Trust propagation: min-cost path through a social graph.
    /// SELECT pg_ripple.load_rules($$
    ///     ?x <ex:trust> ?min_t :-
    ///         ?x <ex:knows> ?y, ?y <ex:trust> ?t1, ?x <ex:directTrust> ?t2,
    ///         COUNT(?z WHERE ?z <ex:knows> ?y) AS min_t = LEAST(?t1, ?t2) .
    /// $$, 'trust_rules');
    /// SELECT pg_ripple.infer_lattice('trust_rules', 'min');
    /// ```
    #[pg_extern]
    fn infer_lattice(
        rule_set: default!(&str, "'custom'"),
        lattice_name: default!(&str, "'min'"),
    ) -> pgrx::JsonB {
        pgrx::JsonB(crate::datalog::run_infer_lattice(rule_set, lattice_name))
    }

    // ── v0.32.0: Tabling / memoisation ───────────────────────────────────────

    /// Return statistics for the tabling / memoisation cache (v0.32.0).
    ///
    /// Each row has:
    /// - `goal_hash BIGINT` — XXH3-64 hash of the cached goal string
    /// - `hits BIGINT` — number of cache hits for this entry
    /// - `computed_ms FLOAT` — wall-clock time (ms) for the original computation
    /// - `cached_at TIMESTAMPTZ` — when the entry was last written
    #[pg_extern]
    fn tabling_stats() -> TableIterator<
        'static,
        (
            name!(goal_hash, i64),
            name!(hits, i64),
            name!(computed_ms, f64),
            name!(cached_at, String),
        ),
    > {
        let rows = crate::datalog::tabling_stats_impl();
        TableIterator::new(rows)
    }

    /// Check all active constraint rules and return violations as JSONB.
    ///
    /// Each element has fields: `rule` (text), `violated` (bool).
    /// Pass `rule_set` to check only that rule set; pass NULL to check all.
    #[pg_extern]
    fn check_constraints(rule_set: default!(Option<&str>, "NULL")) -> pgrx::JsonB {
        let violations = crate::datalog::check_all_constraints(rule_set);
        pgrx::JsonB(serde_json::Value::Array(
            violations.into_iter().map(|v| v.0).collect(),
        ))
    }

    /// Prewarm the hot dictionary table by copying short IRIs and predicates.
    ///
    /// Returns the number of rows in the hot table after prewarm.
    #[pg_extern]
    fn prewarm_dictionary_hot() -> i64 {
        crate::dictionary::hot::ensure_hot_table();
        crate::dictionary::hot::prewarm_hot_table();
        pgrx::Spi::get_one::<i64>("SELECT count(*) FROM _pg_ripple.dictionary_hot")
            .unwrap_or(None)
            .unwrap_or(0)
    }

    // ── v0.40.0: explain_datalog ──────────────────────────────────────────────

    /// Return a JSONB explain document for a named Datalog rule set.
    ///
    /// Keys:
    /// - `"strata"` — per-stratum dependency graph with predicate IDs and rule count
    /// - `"rules"` — rewritten rule texts stored in the catalog
    /// - `"sql_per_rule"` — compiled SQL for each rule
    /// - `"last_run_stats"` — per-iteration delta row counts from the last `infer()` run
    #[pg_extern]
    fn explain_datalog(rule_set_name: &str) -> pgrx::JsonB {
        crate::datalog::explain::explain_datalog(rule_set_name)
    }
}
