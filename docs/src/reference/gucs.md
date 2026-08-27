# GUCs

See the [complete GUC reference](guc-reference.md).

`pg_ripple.llm_api_key_env` accepts only names matching
`^[A-Z_][A-Z0-9_]*$`. Raw values are rejected. A superuser may temporarily set
`pg_ripple.llm_api_key_env_allow_raw = on` while migrating from pre-0.131.
