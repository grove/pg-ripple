fn main() {
    // pgrx macros (pg_shmem_init!, etc.) emit cfg(feature = "pgNN") checks for
    // all supported PostgreSQL versions.  We only enable pg18, but Rust 2024's
    // check-cfg linting requires the other values to be declared as expected.
    for ver in ["pg13", "pg14", "pg15", "pg16", "pg17"] {
        println!("cargo::rustc-check-cfg=cfg(feature, values(\"{ver}\"))");
    }
}
