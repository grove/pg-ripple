# v0.132.0 soak-test evidence

The release gate consumes the JSONL output from `benchmarks/soak_72h.sh`.
Run it with `SOAK_HOURS=72` and attach the resulting file to the release.

The gate requires less than 0.01% error rate and less than 10% memory growth
between the first and final samples. A run without a complete 72-hour output
is evidence-incomplete and must not be described as a passing soak.
