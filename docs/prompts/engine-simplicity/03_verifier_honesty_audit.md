# Verifier Honesty Audit

Review whether the verifier itself can be fooled while claiming that
`BOON_DD_ENGINE_SIMPLICITY_PLAN.md` is complete.

Required checks:

- inspect every `cargo xtask verify-*` gate added for the engine-simplicity plan;
- confirm `cargo xtask verify all --format json` includes every required gate;
- confirm reports include input hashes, artifact hashes, git head, dirty status,
  command records, verdicts, failures, and blockers;
- confirm stale generated artifacts fail verification;
- confirm wrong source ids and wrong dynamic owners fail negative tests;
- confirm browser/native proof cannot pass from precomputed JSON, smoke strings,
  checked-example fixture output, or a native graph-worker bridge;
- confirm prompt-audit failures or inconclusive results block final success;
- intentionally look for a minimal code or artifact change that would fake a
  pass, then report whether the verifier catches it.

Return:

- `verdict`: `pass`, `fail`, or `inconclusive`;
- exact fake-pass attempts and results;
- file/line evidence for verifier weaknesses;
- deterministic artifact paths inspected;
- required gate additions or fixes.
