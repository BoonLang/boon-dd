# Runtime Boundary Audit

Trace at least one source event from each host entrypoint through generated graph
execution into monitor, render, effect, and persistence outputs.

Hosts to inspect:

- terminal
- native window
- browser/WASM/Firefox

Fail if a host evaluates Boon expressions, branches on app values, counts app
events, constructs app output text, skips scenario commands, or calls a smoke
runtime helper instead of injecting typed source facts into the generated
Timely/Differential graph and draining structured outputs.

Return the required prompt audit JSON schema with path and line evidence.
