//! SPARQL execute sub-module: exec_core (v0.90.0 CQ-02 pre-emptive split stub).

/// Typed bindings are lowered during planning; execution remains the existing
/// integer-only SPI path. This marker keeps the boundary explicit.
pub(crate) const TYPED_BINDINGS_USE_INTEGER_VALUES: bool = true;
