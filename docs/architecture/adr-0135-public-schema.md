# ADR: keep the `pg_ripple` public schema in v0.135.0

Status: accepted

The v0.135.0 stable API keeps the existing `pg_ripple` schema. Renaming it to
`ripple` would break every qualified function call, generated extension object,
HTTP query, and upgrade path at once. The extension therefore keeps the schema
name and documents the required PostgreSQL `allow_system_table_mods` setting
for installations that need it.

The internal `_pg_ripple` schema remains private. New stable functions and
catalogs follow the existing split. Clients can adopt the v1 manifest without a
namespace rewrite, and a future major release may revisit the public name with
an explicit migration and compatibility window.
