# Generic JSON key → Schema.org property heuristic alignment rules
# ─────────────────────────────────────────────────────────────────
#
# Maps common generic JSON field names to Schema.org property IRIs.
# Useful as a starting point when data sources use unnamespaced keys.
#
# Usage:
#   SELECT pg_ripple.load_vocab_template('generic_to_schema');
#   SELECT pg_ripple.infer('generic_to_schema');
#
# Schema.org prefix: https://schema.org/
#
# NOTE: These are heuristic rules; customise them per deployment.
#       Load domain-specific alignment rules after this template to override.

# --- Common identity/name fields ---

aligned_pred(?s, "https://schema.org/name", ?o) :-
    triple(?s, "name", ?o).

aligned_pred(?s, "https://schema.org/name", ?o) :-
    triple(?s, "title", ?o).

aligned_pred(?s, "https://schema.org/name", ?o) :-
    triple(?s, "label", ?o).

aligned_pred(?s, "https://schema.org/identifier", ?o) :-
    triple(?s, "id", ?o).

aligned_pred(?s, "https://schema.org/identifier", ?o) :-
    triple(?s, "identifier", ?o).

# --- Description fields ---

aligned_pred(?s, "https://schema.org/description", ?o) :-
    triple(?s, "description", ?o).

aligned_pred(?s, "https://schema.org/description", ?o) :-
    triple(?s, "summary", ?o).

# --- Date/time fields ---

aligned_pred(?s, "https://schema.org/dateCreated", ?o) :-
    triple(?s, "created_at", ?o).

aligned_pred(?s, "https://schema.org/dateCreated", ?o) :-
    triple(?s, "createdAt", ?o).

aligned_pred(?s, "https://schema.org/dateModified", ?o) :-
    triple(?s, "updated_at", ?o).

aligned_pred(?s, "https://schema.org/dateModified", ?o) :-
    triple(?s, "updatedAt", ?o).

# --- Contact fields ---

aligned_pred(?s, "https://schema.org/email", ?o) :-
    triple(?s, "email", ?o).

aligned_pred(?s, "https://schema.org/email", ?o) :-
    triple(?s, "email_address", ?o).

aligned_pred(?s, "https://schema.org/telephone", ?o) :-
    triple(?s, "phone", ?o).

aligned_pred(?s, "https://schema.org/telephone", ?o) :-
    triple(?s, "telephone", ?o).

# --- URL fields ---

aligned_pred(?s, "https://schema.org/url", ?o) :-
    triple(?s, "url", ?o).

aligned_pred(?s, "https://schema.org/url", ?o) :-
    triple(?s, "website", ?o).

aligned_pred(?s, "https://schema.org/url", ?o) :-
    triple(?s, "link", ?o).

# --- Address fields ---

aligned_pred(?s, "https://schema.org/addressCountry", ?o) :-
    triple(?s, "country", ?o).

aligned_pred(?s, "https://schema.org/addressLocality", ?o) :-
    triple(?s, "city", ?o).

aligned_pred(?s, "https://schema.org/postalCode", ?o) :-
    triple(?s, "postal_code", ?o).

aligned_pred(?s, "https://schema.org/postalCode", ?o) :-
    triple(?s, "zip", ?o).

# --- Status/type fields ---

aligned_pred(?s, "https://schema.org/status", ?o) :-
    triple(?s, "status", ?o).

aligned_pred(?s, "https://schema.org/additionalType", ?o) :-
    triple(?s, "type", ?o).

aligned_pred(?s, "https://schema.org/additionalType", ?o) :-
    triple(?s, "category", ?o).

# --- Numeric/measurement fields ---

aligned_pred(?s, "https://schema.org/value", ?o) :-
    triple(?s, "value", ?o).

aligned_pred(?s, "https://schema.org/amount", ?o) :-
    triple(?s, "amount", ?o).

aligned_pred(?s, "https://schema.org/price", ?o) :-
    triple(?s, "price", ?o).

aligned_pred(?s, "https://schema.org/quantity", ?o) :-
    triple(?s, "quantity", ?o).

# Emit aligned triples
triple(?s, ?p, ?o) :-
    aligned_pred(?s, ?p, ?o).
