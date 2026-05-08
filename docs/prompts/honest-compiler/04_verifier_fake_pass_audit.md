# Verifier Fake-Pass Audit

Attack the verification harness. Look for ways `cargo xtask verify all --format
json` could pass while the compiler is not honest.

Specifically check for:

- stale generated artifacts
- generated snapshots created by shortcut execution
- only first scenario step being executed
- command/effect/persistence actions being skipped
- string-only checks for browser/native proof
- tests that assert only "some output exists"
- reports that omit hashes or dirty checkout state
- missing negative tests for verifier self-fault injection

Return the required prompt audit JSON schema. Use `fail` for any fake-pass path
that is not blocked by a deterministic gate.
