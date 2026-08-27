//! Queue-facing writeback operations.
//!
//! The queue implementation remains next to the writeback executor so the
//! existing worker call path stays unchanged; this module is the stable
//! subsystem boundary for queue APIs.

pub use super::writeback::{drain_json_writeback_queue, json_writeback_status_impl};
