//! cargo-fuzz target: TriG bulk-loader input (FUZZ-BULK-01, v0.83.0).
//!
//! Feeds arbitrary byte sequences through the TriG parser independently
//! of the PostgreSQL extension context. Asserts: no panic on any input.
//! Invalid syntax must produce a parse error, never a crash.
//!
//! # Running locally
//!
//! ```sh
//! cargo install cargo-fuzz
//! cargo fuzz run trig_load -- -max_total_time=600
//! ```

#![no_main]

use libfuzzer_sys::fuzz_target;
use rio_api::parser::QuadsParser;
use rio_turtle::{TriGParser, TurtleError};

fuzz_target!(|data: &[u8]| {
    let mut parser = TriGParser::new(data, None);
    let _ = parser.parse_all(&mut |_| -> Result<(), TurtleError> { Ok(()) });
});
