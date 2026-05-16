# Pure DD Architecture Audit

Review the current checkout against `BOON_DD_ENGINE_SIMPLICITY_PLAN.md` and
`BOON_DD_HONEST_COMPILER_PLAN.md`.

Determine whether terminal, native, browser, and generated execution paths are
genuinely pure Timely/Differential Dataflow rather than fixture dispatch,
host-side Boon semantics, or smoke output shortcuts.

Required checks:

- trace backend entrypoints to the generated graph execution path;
- find any selection by example name, source path, source text, generated crate
  registry, or smoke fixture;
- verify source events are keyed by compiler-assigned source ids;
- verify dynamic owner and generation identity are preserved into DD keys and
  outputs;
- verify `LATEST`, `HOLD`, keyed hold, list operations, text input, timers, and
  command/effect semantics lower to DD graph state rather than host replay;
- verify browser proof executes the generated Timely/DD graph in the browser
  process.

Return:

- `verdict`: `pass`, `fail`, or `inconclusive`;
- file/line evidence for every violation;
- deterministic artifact paths inspected;
- any path that could still fake a pass.
