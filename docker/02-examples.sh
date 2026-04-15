#!/usr/bin/env bash
# 02-examples.sh
# Creates the "examples" database, installs pg_ripple, and pre-loads a small
# FOAF-style dataset so users can run the sample queries from the Playground
# documentation straight away.

set -euo pipefail

# Create the examples database
psql -v ON_ERROR_STOP=1 \
     --username "$POSTGRES_USER" \
     --dbname   postgres \
     <<-'SQL'
    CREATE DATABASE examples;
SQL

# Install the extension and load data in one psql session
psql -v ON_ERROR_STOP=1 \
     --username "$POSTGRES_USER" \
     --dbname   examples \
     <<-'SQL'
CREATE EXTENSION pg_ripple;

-- ── FOAF example dataset ────────────────────────────────────────────────────
-- Five people (Alice, Bob, Carol, Dave, Eve), two organisations (Acme Corp,
-- Widgets Inc).  The knows graph is intentionally non-trivial so that
-- property-path (knows+) queries return more than one hop.
--
-- Alice  → knows → Bob, Carol
-- Bob    → knows → Dave
-- Carol  → knows → Eve
-- Dave   → knows → (nobody)
-- Eve    → knows → (nobody)
--
-- Memberships: Alice/Bob/Eve → Acme Corp; Carol/Dave → Widgets Inc

SELECT pg_ripple.load_ntriples($NT$
<https://example.org/alice>   <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>  <https://xmlns.com/foaf/0.1/Person> .
<https://example.org/alice>   <https://xmlns.com/foaf/0.1/name>                  "Alice Smith" .
<https://example.org/alice>   <https://xmlns.com/foaf/0.1/knows>                 <https://example.org/bob> .
<https://example.org/alice>   <https://xmlns.com/foaf/0.1/knows>                 <https://example.org/carol> .
<https://example.org/alice>   <https://xmlns.com/foaf/0.1/member>                <https://example.org/acme> .

<https://example.org/bob>     <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>  <https://xmlns.com/foaf/0.1/Person> .
<https://example.org/bob>     <https://xmlns.com/foaf/0.1/name>                  "Bob Jones" .
<https://example.org/bob>     <https://xmlns.com/foaf/0.1/knows>                 <https://example.org/dave> .
<https://example.org/bob>     <https://xmlns.com/foaf/0.1/member>                <https://example.org/acme> .

<https://example.org/carol>   <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>  <https://xmlns.com/foaf/0.1/Person> .
<https://example.org/carol>   <https://xmlns.com/foaf/0.1/name>                  "Carol White" .
<https://example.org/carol>   <https://xmlns.com/foaf/0.1/knows>                 <https://example.org/eve> .
<https://example.org/carol>   <https://xmlns.com/foaf/0.1/member>                <https://example.org/widgets> .

<https://example.org/dave>    <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>  <https://xmlns.com/foaf/0.1/Person> .
<https://example.org/dave>    <https://xmlns.com/foaf/0.1/name>                  "Dave Brown" .
<https://example.org/dave>    <https://xmlns.com/foaf/0.1/member>                <https://example.org/widgets> .

<https://example.org/eve>     <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>  <https://xmlns.com/foaf/0.1/Person> .
<https://example.org/eve>     <https://xmlns.com/foaf/0.1/name>                  "Eve Green" .
<https://example.org/eve>     <https://xmlns.com/foaf/0.1/member>                <https://example.org/acme> .

<https://example.org/acme>    <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>  <https://xmlns.com/foaf/0.1/Organization> .
<https://example.org/acme>    <https://xmlns.com/foaf/0.1/name>                  "Acme Corp" .

<https://example.org/widgets> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>  <https://xmlns.com/foaf/0.1/Organization> .
<https://example.org/widgets> <https://xmlns.com/foaf/0.1/name>                  "Widgets Inc" .
$NT$);
SQL

echo "pg_ripple: examples database ready."
