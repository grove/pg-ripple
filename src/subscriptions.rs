//! SPARQL subscription catalog and notification helpers (v0.73.0, SUB-01).
//!
//! `pg_ripple.subscribe_sparql(id, query, graph_iri)` registers a subscription
//! in `_pg_ripple.sparql_subscriptions`.  After each graph write, the mutation
//! journal flush calls `notify_affected_subscriptions()` which re-executes the
//! registered SPARQL query and notifies listening clients via `pg_notify`.
//!
//! The payload limit of `pg_notify` is 8 KB; when the JSON-serialised result
//! exceeds that limit, a `{"changed": true}` signal is sent instead.

use pgrx::prelude::*;

#[pgrx::pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    /// Register a SPARQL SELECT subscription.
    ///
    /// Stores the subscription in `_pg_ripple.sparql_subscriptions`.
    /// Raises an error when `subscription_id` already exists.
    #[pg_extern]
    pub fn subscribe_sparql(
        subscription_id: &str,
        query: &str,
        graph_iri: default!(Option<&str>, "NULL"),
    ) {
        let trimmed = query.trim().to_ascii_lowercase();
        if !trimmed.starts_with("select") {
            pgrx::error!("subscribe_sparql: only SPARQL SELECT queries are supported");
        }

        let rows = Spi::get_one_with_args::<i64>(
            "INSERT INTO _pg_ripple.sparql_subscriptions \
             (subscription_id, query, graph_iri) VALUES ($1, $2, $3) \
             ON CONFLICT (subscription_id) DO NOTHING \
             RETURNING 1",
            &[
                pgrx::datum::DatumWithOid::from(subscription_id),
                pgrx::datum::DatumWithOid::from(query),
                pgrx::datum::DatumWithOid::from(graph_iri),
            ],
        )
        .unwrap_or(None);

        if rows.is_none() {
            pgrx::error!(
                "subscription {:?} already exists; call unsubscribe_sparql() first",
                subscription_id
            );
        }
    }

    /// Unregister a SPARQL subscription. Silently succeeds if not found.
    #[pg_extern]
    pub fn unsubscribe_sparql(subscription_id: &str) {
        Spi::run_with_args(
            "DELETE FROM _pg_ripple.sparql_subscriptions WHERE subscription_id = $1",
            &[pgrx::datum::DatumWithOid::from(subscription_id)],
        )
        .unwrap_or_else(|e| pgrx::warning!("unsubscribe_sparql: {e}"));
    }

    /// List all registered SPARQL subscriptions.
    #[pg_extern]
    #[allow(clippy::type_complexity)]
    pub fn list_sparql_subscriptions() -> TableIterator<
        'static,
        (
            name!(subscription_id, String),
            name!(query, String),
            name!(graph_iri, Option<String>),
        ),
    > {
        let rows: Vec<(String, String, Option<String>)> = Spi::connect(|c| {
            let mut out = Vec::new();
            let tup_table = c.select(
                "SELECT subscription_id, query, graph_iri \
                 FROM _pg_ripple.sparql_subscriptions ORDER BY created_at",
                None,
                &[],
            )?;
            for row in tup_table {
                let id = row["subscription_id"]
                    .value::<String>()?
                    .unwrap_or_default();
                let q = row["query"].value::<String>()?.unwrap_or_default();
                let g = row["graph_iri"].value::<String>()?;
                out.push((id, q, g));
            }
            Ok::<_, pgrx::spi::Error>(out)
        })
        .unwrap_or_default();

        TableIterator::new(rows)
    }
}

/// Called from the mutation journal flush path to fire any affected subscriptions.
#[allow(dead_code)]
pub fn notify_affected_subscriptions(affected_graph_ids: &[i64]) {
    let graph_iris: Vec<String> = affected_graph_ids
        .iter()
        .filter_map(|&gid| {
            if gid == 0 {
                Some(String::new())
            } else {
                Spi::get_one_with_args::<String>(
                    "SELECT value FROM _pg_ripple.dictionary WHERE id = $1",
                    &[pgrx::datum::DatumWithOid::from(gid)],
                )
                .unwrap_or(None)
            }
        })
        .collect();

    let subscriptions: Vec<(String, String, Option<String>)> = Spi::connect(|c| {
        let mut out = Vec::new();
        let tup_table = c.select(
            "SELECT subscription_id, query, graph_iri FROM _pg_ripple.sparql_subscriptions",
            None,
            &[],
        )?;
        for row in tup_table {
            let id = row["subscription_id"]
                .value::<String>()?
                .unwrap_or_default();
            let q = row["query"].value::<String>()?.unwrap_or_default();
            let g = row["graph_iri"].value::<String>()?;
            out.push((id, q, g));
        }
        Ok::<_, pgrx::spi::Error>(out)
    })
    .unwrap_or_default();

    for (sub_id, query, sub_graph) in subscriptions {
        let affected = match &sub_graph {
            None => true,
            Some(g) => graph_iris.iter().any(|gi| gi == g),
        };
        if !affected {
            continue;
        }

        let payload = match crate::sparql::sparql_query_to_json(&query) {
            Ok(json) => json,
            Err(e) => {
                pgrx::warning!("subscription {:?}: query error: {}", sub_id, e);
                continue;
            }
        };

        let payload_str = if payload.len() > 7800 {
            r#"{"changed":true}"#.to_string()
        } else {
            payload
        };

        let channel = format!("pg_ripple_subscription_{sub_id}");
        Spi::run_with_args(
            "SELECT pg_notify($1, $2)",
            &[
                pgrx::datum::DatumWithOid::from(channel.as_str()),
                pgrx::datum::DatumWithOid::from(payload_str.as_str()),
            ],
        )
        .unwrap_or_else(|e| pgrx::warning!("pg_notify for subscription {:?}: {e}", sub_id));
    }
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    #[allow(unused_imports)]
    use pgrx::prelude::*;
}
