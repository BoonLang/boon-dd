# Cross-Repo Semantics Audit

Compare this repo's accepted Boon language manifest and implementation against
the other local `~/repos/boon-*` implementations.

Identify syntax, semantics, host behavior, scenario behavior, rendering
protocols, or persistence/effect behavior that those repos support but
`boon-dd` does not yet honestly parse, resolve, type-check, lower to
Timely/Differential, generate, run, and verify.

Return the required prompt audit JSON schema. Use `fail` if the manifest claims
full accepted-language coverage while cross-repo examples or semantics remain
missing.
