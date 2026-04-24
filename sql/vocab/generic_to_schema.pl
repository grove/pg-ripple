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

?s <https://schema.org/name> ?o :- ?s <name> ?o .
?s <https://schema.org/name> ?o :- ?s <title> ?o .
?s <https://schema.org/name> ?o :- ?s <label> ?o .
?s <https://schema.org/identifier> ?o :- ?s <id> ?o .
?s <https://schema.org/identifier> ?o :- ?s <identifier> ?o .

# --- Description fields ---

?s <https://schema.org/description> ?o :- ?s <description> ?o .
?s <https://schema.org/description> ?o :- ?s <summary> ?o .

# --- Date/time fields ---

?s <https://schema.org/dateCreated> ?o :- ?s <created_at> ?o .
?s <https://schema.org/dateCreated> ?o :- ?s <createdAt> ?o .
?s <https://schema.org/dateModified> ?o :- ?s <updated_at> ?o .
?s <https://schema.org/dateModified> ?o :- ?s <updatedAt> ?o .

# --- Contact fields ---

?s <https://schema.org/email> ?o :- ?s <email> ?o .
?s <https://schema.org/email> ?o :- ?s <email_address> ?o .
?s <https://schema.org/telephone> ?o :- ?s <phone> ?o .
?s <https://schema.org/telephone> ?o :- ?s <telephone> ?o .

# --- URL fields ---

?s <https://schema.org/url> ?o :- ?s <url> ?o .
?s <https://schema.org/url> ?o :- ?s <website> ?o .
?s <https://schema.org/url> ?o :- ?s <link> ?o .

# --- Address fields ---

?s <https://schema.org/addressCountry> ?o :- ?s <country> ?o .
?s <https://schema.org/addressLocality> ?o :- ?s <city> ?o .
?s <https://schema.org/postalCode> ?o :- ?s <postal_code> ?o .
?s <https://schema.org/postalCode> ?o :- ?s <zip> ?o .

# --- Status/type fields ---

?s <https://schema.org/status> ?o :- ?s <status> ?o .
?s <https://schema.org/additionalType> ?o :- ?s <type> ?o .
?s <https://schema.org/additionalType> ?o :- ?s <category> ?o .

# --- Numeric/measurement fields ---

?s <https://schema.org/value> ?o :- ?s <value> ?o .
?s <https://schema.org/amount> ?o :- ?s <amount> ?o .
?s <https://schema.org/price> ?o :- ?s <price> ?o .
?s <https://schema.org/quantity> ?o :- ?s <quantity> ?o .

