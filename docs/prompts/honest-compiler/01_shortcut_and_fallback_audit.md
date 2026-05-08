# Shortcut And Fallback Audit

Review the current checkout against `BOON_DD_HONEST_COMPILER_PLAN.md`.

Find any source-text semantic scans, fallback paths, fixture-specific logic,
shortcut runtime execution, app-specific lowerings, smoke-only semantics, or
host-side Boon semantics.

Required output schema:

```json
{
  "prompt_id": "01_shortcut_and_fallback_audit",
  "prompt_hash": "...",
  "repo_state_hash": "...",
  "deterministic_report_hash": "...",
  "verdict": "pass | fail | inconclusive",
  "critical_findings": [],
  "reviewed_files": [],
  "reviewed_artifacts": [],
  "commands_reviewed": []
}
```

Fail if any execution path still uses compiler/runtime shortcuts such as
`TextBehavior`, `execute_static_graph`, `evaluate_text`, `compile_and_run_step`,
or semantic decisions from raw source text.
