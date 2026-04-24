//! GUC parameters for the SHACL validation subsystem.

/// GUC: SHACL validation mode — 'off', 'sync', or 'async'.
/// 'sync' rejects violating triples inline; 'async' queues them for the
/// background validation worker; 'off' disables all SHACL enforcement.
pub static SHACL_MODE: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);
