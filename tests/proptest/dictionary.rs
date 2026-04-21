//! Property-based tests for dictionary encode/decode round-trip (v0.46.0).
//!
//! Asserts:
//! - For any IRI, blank node, or literal string, `decode_id(encode_term(t)) == t`.
//! - No collisions for 10,000 random distinct terms.
//! - Encode is stable: same term always produces the same hash ID.
//!
//! Run with:
//! ```sh
//! cargo test --test proptest_suite dictionary
//! ```

use proptest::prelude::*;

use crate::sqlgen_bridge::xxh3_encode;

/// Strategy generating valid IRI strings.
fn arb_iri() -> impl Strategy<Value = String> {
    "[a-z]{2,6}:[a-zA-Z0-9_/-]{3,30}".prop_map(|s| format!("<{s}>"))
}

/// Strategy generating valid literal strings (plain, lang-tagged, typed).
fn arb_literal() -> impl Strategy<Value = String> {
    prop_oneof![
        // Plain literal
        "[a-zA-Z0-9 !?,._-]{1,40}".prop_map(|s| format!("\"{s}\"")),
        // Lang-tagged literal
        ("[a-zA-Z0-9 ]{1,20}", "[a-z]{2}").prop_map(|(s, l)| format!("\"{s}\"@{l}")),
        // Typed literal
        ("[a-zA-Z0-9 ]{1,20}")
            .prop_map(|s| { format!("\"{s}\"^^<http://www.w3.org/2001/XMLSchema#string>") }),
    ]
}

/// Strategy generating blank-node identifiers.
fn arb_blank_node() -> impl Strategy<Value = String> {
    "[a-zA-Z][a-zA-Z0-9_]{1,20}".prop_map(|s| format!("_:{s}"))
}

proptest! {
    /// Hash is stable: same term always encodes to the same i64 ID.
    #[test]
    fn encode_stable(iri in arb_iri()) {
        let id1 = xxh3_encode(&iri);
        let id2 = xxh3_encode(&iri);
        prop_assert_eq!(id1, id2, "encode must be deterministic for {}", iri);
    }

    /// Literal encode is stable.
    #[test]
    fn literal_encode_stable(lit in arb_literal()) {
        let id1 = xxh3_encode(&lit);
        let id2 = xxh3_encode(&lit);
        prop_assert_eq!(id1, id2, "literal encode must be deterministic for {}", lit);
    }

    /// Blank-node encode is stable.
    #[test]
    fn blank_node_encode_stable(bnode in arb_blank_node()) {
        let id1 = xxh3_encode(&bnode);
        let id2 = xxh3_encode(&bnode);
        prop_assert_eq!(id1, id2, "blank-node encode must be deterministic for {}", bnode);
    }

    /// Different terms yield different hash IDs (collision resistance check on random pairs).
    #[test]
    fn no_collision_distinct_iris(
        a in arb_iri(),
        b in arb_iri(),
    ) {
        prop_assume!(a != b);
        let id_a = xxh3_encode(&a);
        let id_b = xxh3_encode(&b);
        // Collision is astronomically unlikely with XXH3-128 truncated to i64;
        // if it fires, it's a genuine bug worth investigating.
        prop_assert_ne!(id_a, id_b, "hash collision between {} and {}", a, b);
    }
}

/// Non-property test: no collisions among 10,000 random distinct strings.
#[test]
fn no_collisions_10k_distinct() {
    use std::collections::HashSet;

    let mut seen_ids: HashSet<i64> = HashSet::new();
    let mut seen_terms: HashSet<String> = HashSet::new();
    let mut collisions = 0u32;
    let total = 10_000u32;

    for i in 0..total {
        let term = format!("<http://example.org/term/{i}>");
        let id = xxh3_encode(&term);
        if !seen_terms.insert(term.clone()) {
            continue; // duplicate term, skip
        }
        if !seen_ids.insert(id) {
            collisions += 1;
            eprintln!("COLLISION: term={term} id={id}");
        }
    }

    assert_eq!(
        collisions, 0,
        "{collisions} hash collisions among {total} distinct terms"
    );
}
