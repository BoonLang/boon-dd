# Runtime Simplicity And Performance Audit

Review the current checkout against `BOON_DD_ENGINE_SIMPLICITY_PLAN.md` with a
focus on code size, runtime shape, and interaction cost.

Required checks:

- compare handwritten Rust, generated Rust, generated total code/data, and
  branch-like complexity against `engine_start` and `pre_honest_compiler`
  baselines;
- compare the current repo with local sibling `~/repos/boon-*` engines using the
  metrics produced by `cargo xtask compare-engines --format json`;
- trace a native interaction and a browser interaction and confirm they reuse a
  long-lived graph session;
- confirm output drains are incremental and do not clone whole output vectors on
  execution paths;
- inspect stress reports for graph rebuild counts, replay counts, output clone
  counts, retained output growth, and latency regressions;
- identify modules that grew without replacing shortcuts or moving duplicated
  code into shared runtime crates.

Return:

- `verdict`: `pass`, `fail`, or `inconclusive`;
- concise comparison table;
- file/line evidence for any performance or complexity issue;
- artifact paths inspected;
- concrete next fix for each failure.
