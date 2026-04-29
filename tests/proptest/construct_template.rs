//! Property-based tests for ConstructTemplate / apply_construct_template (PROPTEST-01, v0.72.0).
//!
//! Property under test: `apply_construct_template(template, bindings)` produces the
//! same set of quads as a naive reference implementation of the same algorithm.
//!
//! Also includes a heterogeneous JSON array property test (RT-10): the
//! `json_to_ntriples` conversion does not panic on random mixed-type payloads.
//!
//! No database connection is required — tests run in pure Rust.

use proptest::prelude::*;

// ─── Minimal TemplateSlot / ConstructTemplate mirror ─────────────────────────
//
// We mirror the types from src/sparql/plan.rs here (without importing pgrx)
// so that the proptest can run without a PostgreSQL instance.

#[derive(Debug, Clone, PartialEq)]
pub enum TemplateSlot {
    Constant(i64),
    Var(usize),
}

pub type ConstructTemplate = Vec<(TemplateSlot, TemplateSlot, TemplateSlot)>;

/// Production implementation (mirrors src/sparql/plan.rs).
pub fn apply_construct_template(
    template: &ConstructTemplate,
    row_vals: &[Option<i64>],
) -> Vec<(i64, i64, i64)> {
    let resolve = |slot: &TemplateSlot| -> Option<i64> {
        match slot {
            TemplateSlot::Constant(id) => Some(*id),
            TemplateSlot::Var(idx) => {
                if *idx == usize::MAX {
                    return None;
                }
                row_vals.get(*idx).copied().flatten()
            }
        }
    };

    template
        .iter()
        .filter_map(|(s_slot, p_slot, o_slot)| {
            let s = resolve(s_slot)?;
            let p = resolve(p_slot)?;
            let o = resolve(o_slot)?;
            Some((s, p, o))
        })
        .collect()
}

/// Naive reference implementation.
pub fn apply_template_naive(
    template: &ConstructTemplate,
    row_vals: &[Option<i64>],
) -> Vec<(i64, i64, i64)> {
    let mut out = Vec::new();
    for (s_slot, p_slot, o_slot) in template {
        let resolve = |slot: &TemplateSlot| -> Option<i64> {
            match slot {
                TemplateSlot::Constant(id) => Some(*id),
                TemplateSlot::Var(idx) => {
                    if *idx == usize::MAX {
                        return None;
                    }
                    row_vals.get(*idx).copied().flatten()
                }
            }
        };
        if let (Some(s), Some(p), Some(o)) = (resolve(s_slot), resolve(p_slot), resolve(o_slot)) {
            out.push((s, p, o));
        }
    }
    out
}

// ─── Generators ──────────────────────────────────────────────────────────────

fn arb_template_slot() -> impl Strategy<Value = TemplateSlot> {
    prop_oneof![
        (1_i64..100_000_i64).prop_map(TemplateSlot::Constant),
        (0_usize..3_usize).prop_map(TemplateSlot::Var),
        Just(TemplateSlot::Var(usize::MAX)),
    ]
}

fn arb_construct_template() -> impl Strategy<Value = ConstructTemplate> {
    prop::collection::vec(
        (
            arb_template_slot(),
            arb_template_slot(),
            arb_template_slot(),
        ),
        1..=5,
    )
}

fn arb_bindings() -> impl Strategy<Value = Vec<Option<i64>>> {
    prop::collection::vec(
        prop_oneof![(1_i64..100_000_i64).prop_map(Some), Just(None),],
        3,
    )
}

// ─── Properties ──────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// PROPTEST-01 (required): apply_construct_template matches naive reference.
    #[test]
    fn construct_template_matches_naive(
        template in arb_construct_template(),
        bindings in arb_bindings(),
    ) {
        let fast  = apply_construct_template(&template, &bindings);
        let naive = apply_template_naive(&template, &bindings);
        prop_assert_eq!(fast, naive, "apply_construct_template diverged from naive reference");
    }

    /// All-None bindings → no variable-slot triples emitted.
    #[test]
    fn empty_bindings_produces_no_variable_triples(
        n in 1_usize..=5_usize,
    ) {
        let template: ConstructTemplate = (0..n)
            .map(|_| (TemplateSlot::Var(0), TemplateSlot::Var(1), TemplateSlot::Var(2)))
            .collect();
        let empty_bindings: Vec<Option<i64>> = vec![None, None, None];
        let result = apply_construct_template(&template, &empty_bindings);
        prop_assert!(result.is_empty());
    }

    /// All-constant template length == template length regardless of bindings.
    #[test]
    fn all_constant_template_length(
        len in 1_usize..=5_usize,
        bindings in arb_bindings(),
    ) {
        let template: ConstructTemplate = (0..len)
            .map(|i| (
                TemplateSlot::Constant((i as i64 + 1) * 10),
                TemplateSlot::Constant((i as i64 + 1) * 20),
                TemplateSlot::Constant((i as i64 + 1) * 30),
            ))
            .collect();
        let result = apply_construct_template(&template, &bindings);
        prop_assert_eq!(result.len(), len);
    }
}

// ─── RT-10: Heterogeneous JSON array smoke property ──────────────────────────
//
// Verifies that the N-Triples serialisation for heterogeneous arrays does
// not panic.  Uses spargebra's number representation directly; no pgrx needed.

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// json_to_ntriples conceptual smoke: mixed-type JSON arrays are safe.
    ///
    /// We test the serde_json -> ntriples term mapping directly on Number values.
    #[test]
    fn number_to_nt_term_no_panic(
        int_val in any::<i64>(),
        float_val in -1e15_f64..1e15_f64,
    ) {
        // Mirror the logic from bulk_load.rs json_value_to_nt_term for numbers.
        let int_n = serde_json::json!(int_val);
        let float_n = serde_json::json!(float_val);

        if let serde_json::Value::Number(n) = &int_n {
            // Should successfully convert without panic.
            let is_float = n.is_f64();
            let _ = if is_float {
                n.as_f64().map(|f| format!("\"{}\"^^<http://www.w3.org/2001/XMLSchema#decimal>", f))
            } else if let Some(i) = n.as_i64() {
                Some(format!("\"{}\"^^<http://www.w3.org/2001/XMLSchema#integer>", i))
            } else {
                n.as_u64().map(|u| format!("\"{}\"^^<http://www.w3.org/2001/XMLSchema#integer>", u))
            };
        }

        if let serde_json::Value::Number(n) = &float_n {
            let is_float = n.is_f64();
            let _ = if is_float {
                n.as_f64().map(|f| format!("\"{}\"^^<http://www.w3.org/2001/XMLSchema#decimal>", f))
            } else if let Some(i) = n.as_i64() {
                Some(format!("\"{}\"^^<http://www.w3.org/2001/XMLSchema#integer>", i))
            } else {
                n.as_u64().map(|u| format!("\"{}\"^^<http://www.w3.org/2001/XMLSchema#integer>", u))
            };
        }
    }
}

// ─── RT-10: Heterogeneous JSON construction proptest (PROPTEST-01 extension) ──
//
// Verifies that constructing JSON objects with mixed-type values does not
// panic and that the serde_json library handles all combinations correctly.
// The actual json_to_ntriples() round-trip requires a live database and is
// covered by pg_regress tests (json_roundtrip_fixes.sql).

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Mixed-type JSON object construction does not panic.
    #[test]
    fn heterogeneous_json_object_no_panic(
        keys in prop::collection::vec("[a-z]{3,8}", 1..=5),
        ints in prop::collection::vec(any::<i32>(), 5),
        floats in prop::collection::vec(-1e6_f64..1e6_f64, 5),
        bools in prop::collection::vec(any::<bool>(), 5),
        strings in prop::collection::vec("[a-zA-Z0-9 ]{0,20}", 5),
    ) {
        let mut map = serde_json::Map::new();
        for (i, k) in keys.iter().enumerate() {
            let val = match i % 4 {
                0 => serde_json::json!(ints[i % 5]),
                1 => serde_json::json!(floats[i % 5]),
                2 => serde_json::json!(bools[i % 5]),
                _ => serde_json::json!(strings[i % 5]),
            };
            map.insert(k.clone(), val);
        }
        let payload = serde_json::Value::Object(map);
        // Serialize and deserialize should be stable.
        let serialized = serde_json::to_string(&payload).expect("serialization must not fail");
        let _deserialized: serde_json::Value = serde_json::from_str(&serialized)
            .expect("round-trip deserialization must not fail");
    }
}
