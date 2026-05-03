# Boon DD Architecture

This repository follows the DD-first rule from `boon_timely_dd_transpiler_plan.md`.

- Boon semantics are compiled into generated Rust that constructs a static Timely/Differential Dataflow graph.
- Hosts may inject source events, advance inputs, drain probes, and apply render/effect/persist/monitor outputs.
- Hosts must not implement Boon dependency scheduling, dirty-node propagation, or app-specific semantic fallbacks.
- Browser execution must run the generated Timely/Differential graph inside browser-hosted WASM.
- Native and browser window verification must use `cosmic-background-launch` for focus-safe launches.
