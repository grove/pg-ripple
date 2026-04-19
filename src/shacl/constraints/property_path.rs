//! Property path constraint helpers (placeholder for future sh:path expressions).
//!
//! Currently, pg_ripple compiles SHACL property paths to VP table joins at
//! shape-load time.  Complex path expressions (sh:alternativePath, sh:zeroOrMorePath,
//! sh:inversePath) are planned for a future release.

// No constraint checkers in v0.38.0; file exists to satisfy the module boundary.
