# Negative Corpus

This directory is reserved for checked-in negative Boon fixtures required by
`BOON_DD_HONEST_COMPILER_PLAN.md`.

Phase 0 intentionally leaves the corpus incomplete. Later phases must add
syntax, resolver, shape/type, unsupported-lowering, stale-artifact, and
adversarial no-heuristics cases. `cargo xtask verify-negative-corpus --format
json` must fail until those cases exist and are wired into deterministic
verification.
