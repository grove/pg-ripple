// BUILD-TIME-FIELD-01 (v0.83.0): emit an RFC-3339 build timestamp so that
// the /health endpoint can report a real timestamp instead of the package
// version string.
fn main() {
    let ts = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(|epoch| {
            // Minimal RFC-3339 formatter without external deps.
            let secs = epoch;
            let days = secs / 86400;
            let rem = secs % 86400;
            let h = rem / 3600;
            let m = (rem % 3600) / 60;
            let s = rem % 60;
            // Convert days since Unix epoch to Y-M-D (Gregorian proleptic)
            let z = days + 719468;
            let era = z / 146097;
            let doe = z % 146097;
            let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
            let y = yoe + era * 400;
            let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            let mp = (5 * doy + 2) / 153;
            let d = doy - (153 * mp + 2) / 5 + 1;
            let mo = if mp < 10 { mp + 3 } else { mp - 9 };
            let y = if mo <= 2 { y + 1 } else { y };
            format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
        })
        .unwrap_or_else(|| format!("build-version={}", env!("CARGO_PKG_VERSION")));
    println!("cargo::rustc-env=BUILD_TIMESTAMP={ts}");
}
