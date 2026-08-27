//! Trigger installation and lifecycle operations for JSON writeback.

pub use super::writeback::{
    disable_json_writeback_impl, enable_json_writeback_impl,
    install_writeback_triggers_after_promotion,
};
