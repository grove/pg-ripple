//! SPI / database helpers for SHACL shape storage and loading.

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;

use super::Shape;

/// Persist a parsed `Shape` into `_pg_ripple.shacl_shapes` as JSON.
fn store_shape(shape: &Shape) -> Result<(), String> {
    let json = serde_json::to_value(shape)
        .map_err(|e| format!("failed to serialise shape '{}': {e}", shape.shape_iri))?;
    let json_str = json.to_string();

    Spi::run_with_args(
        "INSERT INTO _pg_ripple.shacl_shapes (shape_iri, shape_json, active)
         VALUES ($1, $2::jsonb, true)
         ON CONFLICT (shape_iri) DO UPDATE
             SET shape_json = EXCLUDED.shape_json,
                 active     = true,
                 updated_at = now()",
        &[
            DatumWithOid::from(shape.shape_iri.as_str()),
            DatumWithOid::from(json_str.as_str()),
        ],
    )
    .map_err(|e| format!("failed to store shape '{}': {e}", shape.shape_iri))
}

/// Parse SHACL Turtle data, store each shape, and return the count stored.
pub fn parse_and_store_shapes(data: &str) -> i32 {
    let shapes = match super::parser::parse_shacl_turtle(data) {
        Ok(s) => s,
        Err(e) => pgrx::error!("SHACL shape parsing failed: {e}"),
    };

    if shapes.is_empty() {
        pgrx::warning!("load_shacl: no shapes found in the supplied Turtle data");
        return 0;
    }

    let mut stored = 0i32;
    for shape in &shapes {
        // SHACL-TXN-01 (v0.81.0): wrap the shape-store write in a savepoint so
        // that a constraint failure rolls back only this shape rather than the
        // entire transaction.  populate_hints() runs after the savepoint is
        // released; hint-write failures are non-fatal warnings.
        let sp_name = format!("shacl_txn_sp_{}", shape.shape_iri.len()); // stable but unique-ish name
        let _ = Spi::run_with_args(&format!("SAVEPOINT {sp_name}"), &[]);
        match store_shape(shape) {
            Ok(()) => {
                let _ = Spi::run_with_args(&format!("RELEASE SAVEPOINT {sp_name}"), &[]);
                stored += 1;
                // Populate DDL hints outside the savepoint (non-fatal if it fails).
                super::hints::populate_hints(shape);
            }
            Err(e) => {
                let _ = Spi::run_with_args(&format!("ROLLBACK TO SAVEPOINT {sp_name}"), &[]);
                pgrx::error!("failed to store shape '{}': {e}", shape.shape_iri);
            }
        }
    }

    let infer_mode = crate::INFERENCE_MODE
        .get()
        .as_ref()
        .and_then(|c| c.to_str().ok())
        .unwrap_or("off")
        .to_owned();

    if infer_mode != "off" {
        let registered = super::af_rules::bridge_shacl_rules(data);
        if registered > 0 {
            pgrx::warning!(
                "load_shacl: auto-registered {registered} sh:rule entries as Datalog rules"
            );
        }
    }

    stored
}

/// Load all active shapes from `_pg_ripple.shacl_shapes`.
pub fn load_shapes() -> Vec<Shape> {
    let rows = Spi::connect(|c| {
        let tup = c
            .select(
                "SELECT shape_json::text FROM _pg_ripple.shacl_shapes WHERE active = true",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("load_shapes SPI error: {e}"));
        let mut out: Vec<String> = Vec::new();
        for row in tup {
            if let Ok(Some(s)) = row.get::<&str>(1) {
                out.push(s.to_owned());
            }
        }
        out
    });

    rows.into_iter()
        .filter_map(|json| serde_json::from_str::<Shape>(&json).ok())
        .collect()
}
