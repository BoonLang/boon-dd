# Boon Timely/Differential Dataflow Transpiler Plan

This file is an implementation brief for Codex CLI / implementation agents. Treat it as the source of truth for this repository unless a human maintainer changes it.

The goal is to replace the painful custom Rust scheduler direction with a generated, static Timely/Differential Dataflow graph. Boon should compile to Rust code that **constructs a Timely/DD graph**, not Rust code that manually schedules dirty nodes.

The final deliverable is a set of playgrounds that run the important Boon examples in three hosts:

1. **Terminal playground** using Ratatui.
2. **Native window playground** using `app_window` + `wgpu` + WESL/WGSL shaders.
3. **Browser window playground** using browser-hosted WASM + WebGPU rendering. The generated Timely/Differential graph must run in the browser process. No native graph-worker bridge, no browser-side custom scheduler, and no semantic fallback are allowed. If Timely/DD cannot compile or run in browser-hosted WASM, fail fast and fix/pin/fork the dependency before proceeding.

The important examples are:

- `counter`
- `counter_hold`
- `interval`
- `interval_hold`
- `pong`
- `todo_mvc`
- `todo_mvc_physical`
- `cells`
- `latest`
- `when`
- `while`
- `then`
- list examples: `list_map_block`, `list_map_external_dep`, `list_object_state`, `list_retain_count`, `list_retain_reactive`, `list_retain_remove`
- app examples: `shopping_list`, `crud`, `flight_booker`, `temperature_converter`

Do not make the compiler know about any specific example. The examples must work because the Boon core and library metadata are general.

---

## Unattended `/goal` contract

The implementation goal is complete only when local verification passes. Continuous integration is not required for this plan.

Required local commands:

```bash
cargo xtask bootstrap --check
cargo xtask verify-deps --format json
cargo xtask verify-wasm-dd --required --browser firefox
cargo xtask verify all --format json
```

Required success artifacts:

```text
target/boon-artifacts/verify-report.json
target/boon-artifacts/success.json
```

`success.json` must report `success: true`, zero failed gates, the exact dependency/tool versions, the canonical example matrix results, and the forbidden-pattern scan result.

An unattended agent may stop only in one of two states:

1. `cargo xtask verify all --format json` passes locally and writes `success.json` with `success: true`.
2. A hard blocker prevents the full goal, and the agent writes a checked-in blocker report under `docs/blockers/` with the failing command, exact output, dependency revisions, minimized repro, and next pin/fork/fix decision.

Browser-hosted Timely/Differential WASM is part of the final deliverable. If it fails, the full `/goal` is blocked; do not treat terminal/native progress as a complete goal.

All commands that open native or browser windows during verification must launch the actual window-creating process through:

```bash
cosmic-background-launch --workspace boon-dd -- <command> [args...]
```

Before any native/browser GUI verification, `xtask` must check:

```bash
command -v cosmic-background-launch
busctl --user list | rg 'com\.system76\.CosmicComp\.BackgroundLaunch'
```

If the helper or live COSMIC background-launch service is unavailable, native/browser GUI verification must fail with a clear local blocker instead of opening foreground windows. Wrapping a long bootstrap command is not enough; the helper must be applied to the process that creates the native window or Firefox/browser window so it does not steal focus or change the user’s current workspace layout.

---

## 0. Design decision summary

### 0.1 Use Timely/DD as the scheduler

Do not implement a custom Boon dirty-node scheduler.

The compiler should generate Rust graph-construction code like this:

```rust
pub fn build_dataflow(worker: &mut timely::worker::Worker<...>) -> GeneratedAppGraph {
    worker.dataflow::<BoonTime, _, _>(|scope| {
        let sources = boon_dd::Sources::new(scope);

        let increment_press = sources.source_leaf::<EmptyRecord>(SourceId::IncrementPress);

        let one = boon_dd::then_const(
            NodeId::CounterThenOne,
            &increment_press,
            1_i64,
        );

        let counter = boon_std::math::sum(
            NodeId::CounterSum,
            &one,
        );

        boon_std::document::render_text(
            NodeId::Document,
            &counter,
        );

        boon_dd::monitor::tap_cell(NodeId::Counter, &counter);
        boon_dd::persist::tap_cell(StorageKey::Counter, &counter);

        sources.finish()
    })
}
```

The compiler must **not** generate Rust event handlers like this:

```rust
fn on_increment_press(&mut self) {
    self.counter += 1;
    self.mark_dirty(NodeId::Counter);
    self.recompute_dependents();
}
```

The host event loop may inject source facts and step Timely probes. That is allowed. The host event loop must not perform Boon dependency scheduling.

### 0.2 Boon logic stays in Boon and libraries

The transpiler may know:

- Boon syntax.
- Name resolution.
- Structural shape inference.
- `SOURCE` inference.
- Core combinators: `LATEST`, `WHEN`, `THEN`, `WHILE`, `HOLD`, `LIST`, `BLOCK`, pipes, field reads, records, text, tags.
- Ownership scopes and stable keys.
- How to generate static Timely/DD graph construction code.
- Library metadata: purity, source bindings, effect commands, persistence policy, render schema, DD-lowerability.

The transpiler must not know:

- Counter logic.
- TodoMVC logic.
- Pong physics.
- Element rendering internals.
- Theme/material internals.
- File/router/timer internals beyond host metadata.
- Any app-specific dependency rule.

### 0.3 `SOURCE` has no explicit type

Use only:

```boon
SOURCE
```

Never introduce:

```boon
SOURCE(Press)
SOURCE(KeyDown)
SOURCE(TextChange)
```

The compiler infers source leaf shape from host bindings and logic use.

Important correction: **key-down does not carry text**. Text input state comes from a text source/current text cell. Key-down carries only the key shape.

Canonical text-input source record:

```boon
store: [
    sources: [
        new_todo_input: [
            text: SOURCE
            event: [
                key_down: [
                    key: SOURCE
                ]
                focus: SOURCE
                blur: SOURCE
            ]
        ]
    ]
]
```

Canonical button source record:

```boon
store: [
    sources: [
        increment_button: [
            event: [
                press: SOURCE
            ]
            hovered: SOURCE
        ]
    ]
]
```

Elements receive the source record directly:

```boon
Element/button(
    element: store.sources.increment_button
    style: []
    label: counter |> Text/from_number()
)
```

Business logic reads the same paths:

```boon
counter:
    0 |> HOLD state {
        store.sources.increment_button.event.press
        |> THEN { state + 1 }
    }
```

This replaces the old `LINK` and `|> LINK { ... }` pattern.

### 0.4 Use a small Boon-DD kernel, not a custom scheduler

The Boon runtime kernel should be small, generic, and inside Timely/DD:

- `source_leaf`
- `then`
- `then_const`
- `when`
- `while_switch`
- `latest`
- `sample`
- `hold`
- `keyed_hold`
- `list_append`
- `list_remove`
- `list_map`
- `list_retain`
- `render_sink`
- `effect_sink`
- `persist_tap`
- `monitor_tap`

Everything else is library code.

### 0.5 One source emission is one Timely epoch

Use a deterministic timestamp type:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BoonTime {
    pub epoch: u64,
    pub phase: u8,
}
```

Initial phase plan:

```text
phase 0: source facts and persisted bootstrap facts
phase 1: state/HOLD outputs
phase 2: derived values, collection/list outputs
phase 3: render/effect/persist/monitor commands
```

Formal timestamp contract:

- Source events and persisted bootstrap records are injected at `BoonTime { epoch: e, phase: 0 }`.
- `HOLD` and `keyed_hold` outputs for that event are retimed to `BoonTime { epoch: e, phase: 1 }`.
- Derived values, collection/list outputs, `LATEST`, `WHEN`, `THEN`, and `WHILE` outputs are retimed to `BoonTime { epoch: e, phase: 2 }` unless the operator explicitly documents a narrower phase.
- Render, effect, persistence, and monitor command streams are retimed to `BoonTime { epoch: e, phase: 3 }`.
- A host submission is complete only after the probe frontier is no longer less than `BoonTime { epoch: e, phase: 3 }`.
- `THEN` bodies that sample cells read the previous completed epoch snapshot unless an operator explicitly retimes an input to a later phase. This prevents same-epoch feedback from becoming implementation-defined.
- Multiple same-owner updates in one epoch must be ordered by generated branch/order metadata or rejected as a compile error. Do not leave same-epoch ordering to collection iteration order.

The implementation must prove `BoonTime` satisfies a real Timely/DD graph with `map`, `join` or `reduce`, `probe`, `then_const`, and `hold` before building user examples. If a custom timestamp cannot satisfy Timely/Differential progress traits cleanly, encode `(epoch, phase)` into a supported scalar timestamp such as `u64`; keep the public semantics above unchanged.

Start with one Timely worker. Multiple workers are optional later.

---

## 1. Repository direction

The existing `boon-rust` workspace layout is useful as a reference. Implement this plan in this repository. Keep the rough crate layout, but change the purpose of `boon_runtime`: it should contain DD/Timely graph helpers and host boundary adapters, not a custom dirty scheduler.

Recommended workspace:

```text
boon-dd/
  Cargo.toml
  crates/
    boon_syntax/
    boon_hir/
    boon_shape/
    boon_host_schema/
    boon_source/
    boon_dd/
    boon_runtime_host/
    boon_compiler/
    boon_codegen_rust/
    boon_render_ir/
    boon_backend_ratatui/
    boon_backend_wgpu/
    boon_backend_app_window/
    boon_backend_browser/
    boon_examples/
    boon_verify/
    xtask/
  examples/
    counter/
    counter_hold/
    interval/
    interval_hold/
    pong/
    todo_mvc/
    todo_mvc_physical/
    cells/
    latest/
    when/
    while/
    then/
    list_retain_reactive/
    shopping_list/
  shaders/
    common/
    pipelines/
  generated/
    counter/
    counter_hold/
    interval/
    interval_hold/
    pong/
    todo_mvc/
    todo_mvc_physical/
  tests/
    scenarios/
  docs/
```

If existing crate names already exist, do not churn names unless necessary. It is acceptable to implement `boon_dd` as a module inside `boon_runtime` first, but the architecture must be DD-first.

### 1.1 Dependencies

Start with pinned versions unless a dependency gate proves and records a newer compatible set:

```toml
[dependencies]
timely = "=0.29.0"
differential-dataflow = "=0.23.0"
```

Other initial dependencies:

```toml
serde = { version = "1", features = ["derive"] }
serde_json = "1"
smallvec = "1"
indexmap = "2"
tracing = "0.1"
tracing-subscriber = "0.3"
anyhow = "1"
thiserror = "2"
```

Renderer dependencies:

```toml
ratatui = "=0.30.0"
crossterm = "=0.29.0"
wgpu = "=29.0.3"
wesl = "=0.3.2"
wgsl_bindgen = "=0.22.2"
app_window = "=0.3.3"
```

Tooling versions:

```text
wasm-bindgen-cli = 0.2.120
```

The first implementation step must choose and commit a coherent dependency set, `Cargo.lock`, and Rust toolchain file. `cargo xtask verify-deps --format json` must report the exact crate versions, feature flags, Rust version, installed `wasm32-unknown-unknown` target, local wasm tooling, Firefox/WebGPU preflight, WESL-to-WGSL shader compile preflight, and `cargo tree -e features` artifact path.

For browser builds, start with `default-features = false` on Timely/DD where the crates support it. The wasm gate decides the exact working dependency set. Do not move browser execution out of WASM to avoid dependency issues.

### 1.2 Local verification only

Do not require GitHub Actions or any other remote automation for this plan. All compile, test, browser, native-window, shader, and scenario gates are local `xtask` gates. `/goal` readiness and completion are defined only by local commands and local artifacts.

### 1.3 Focus-safe native/browser launch

Native window and browser playground commands must be launched through `cosmic-background-launch --workspace boon-dd -- ...` when they create GUI windows. `xtask` must keep the helper close to the actual GUI phase so the environment variable and compositor launch id are inherited by the process that creates the window.

Use these command shapes for interactive/manual launches:

```bash
cosmic-background-launch --workspace boon-dd -- cargo xtask run --example counter --target native
cosmic-background-launch --workspace boon-dd -- cargo xtask run --example counter --target browser
```

For automated verification, prefer non-interactive/headless assertions when possible. When Firefox or a native window must open, the verifier must use `cosmic-background-launch --workspace boon-dd -- ...` and fail if the helper or user D-Bus service is unavailable.

---

## 2. Internal flow model

Boon user code does not expose a hard event/value split, but the compiler/runtime may use internal flow classes.

Use these internal wrappers:

```rust
pub struct Event<S, T> {
    pub stream: timely::dataflow::Stream<S, EventRecord<T>>,
}

pub struct Cell<S, K, V> {
    pub collection: differential_dataflow::Collection<S, (K, V), Diff>,
}

pub struct CollectionFlow<S, K, V> {
    pub collection: differential_dataflow::Collection<S, (K, V), Diff>,
}

pub struct Command<S, T> {
    pub stream: timely::dataflow::Stream<S, CommandRecord<T>>,
}
```

Where:

```rust
pub type Diff = isize;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OwnerKey(/* generated or interned owner path */);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceId(/* generated stable id */);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(/* generated stable id */);
```

Scalar top-level cells are singleton keyed cells:

```text
Cell<(), V>
```

Row-local cells are keyed cells:

```text
Cell<TodoId, V>
```

A list is a keyed collection plus an order/index value:

```text
Collection<ListKey, ListItem>
```

---

## 3. Shape inference

Keep shape inference structural and deterministic.

Shapes:

```rust
pub enum Shape {
    Unknown,
    EmptyRecord,
    Record(BTreeMap<FieldName, Shape>),
    List(Box<Shape>),
    Text,
    Number(NumberKind),
    TagSet(BTreeSet<TagName>),
    Function(FunctionShape),
    SourceMarker,
    Skip,
    Union(Vec<Shape>),
}
```

Rules:

- `Unknown` must not survive final validation.
- `SOURCE` starts as `SourceMarker`.
- Host schema binds `SOURCE` leaves to concrete shapes.
- Logic reads impose shape constraints.
- Conflicts are compile errors.
- Do not add user-visible nominal source types.
- Do not invent `Bool` syntax. Internally represent booleans as `TagSet { False, True }`.
- Press/click/blur/focus can use `EmptyRecord` emissions.
- Key down uses `TagSet` under `.key`, not `.text`.
- Text input current text uses `.text` as a text cell/source.

### 3.1 Canonical value and wire schema

Generated code and verification artifacts must use one canonical value model:

```rust
pub enum BoonValue {
    EmptyRecord,
    Record(BTreeMap<FieldName, BoonValue>),
    List(Vec<BoonValue>),
    Text(String),
    Number(BoonNumber),
    Tag { name: TagName, payload: Option<Box<BoonValue>> },
}

pub enum BoonNumber {
    Int(i64),
    Float(OrderedFloat<f64>),
}
```

Rules:

- User-facing booleans are tags: `True` and `False`. Generated source payloads must not use raw `bool` unless the field is a private Rust optimization that serializes as `Tag { True | False }`.
- Key tags with text payloads use `Tag { name: Character, payload: Text(...) }`; non-character keys use tags without payloads.
- `SKIP` is not a value. It means “emit no record” in event/collection flows.
- JSON serialization, monitor previews, persisted values, and scenario fixtures must use the same canonical shape names and field ordering.
- Shape hashes are computed from canonical shape JSON with sorted map keys, not from Rust type names.

Example host schema:

```rust
Element/button(element):
  element.event.press -> EmptyRecord optional
  element.hovered     -> TagSet { False, True } optional
  element.focused     -> TagSet { False, True } optional

Element/text_input(element):
  element.text               -> Text optional
  element.event.change       -> EmptyRecord optional
  element.event.key_down.key -> TagSet { Enter, Escape, Backspace, Character, Other, ... } optional
  element.event.blur         -> EmptyRecord optional
  element.event.focus        -> EmptyRecord optional

Element/checkbox(element):
  element.event.click -> EmptyRecord optional
  element.checked     -> TagSet { False, True } optional
  element.hovered     -> TagSet { False, True } optional

Element/label(element):
  element.event.double_click -> EmptyRecord optional
  element.hovered            -> TagSet { False, True } optional
```

For text input, a `change` event can be used as a trigger, but text is read from `element.text`:

```boon
new_todo_text:
    Text/empty() |> HOLD text {
        LATEST {
            store.sources.new_todo_input.text

            title_to_add |> THEN {
                Text/empty()
            }
        }
    }
```

---

## 4. SOURCE resolution

Every `SOURCE` leaf resolves to one of:

```rust
pub enum ResolvedSource {
    Static {
        source_id: SourceId,
        path: SourcePath,
        shape: Shape,
    },
    DynamicFamily {
        family_id: SourceFamilyId,
        owner_template: OwnerTemplateId,
        item_key_shape: Shape,
        path: SourcePath,
        shape: Shape,
    },
}
```

Static source example:

```boon
store.sources.increment_button.event.press
```

Dynamic source family example inside `new_todo`:

```boon
FUNCTION new_todo(title) {
    [
        id: Ulid/generate()
        sources: [
            checkbox: [event: [click: SOURCE]]
            remove_button: [event: [press: SOURCE]]
            edit_input: [
                text: SOURCE
                event: [key_down: [key: SOURCE], blur: SOURCE]
            ]
        ]
        ...
    ]
}
```

This becomes one canonical generated event schema. Rust enum variants may be generated for ergonomics, but they must serialize and inject through this shape:

```rust
pub enum GeneratedSourceEventPayload {
    EmptyRecord,
    Text(String),
    Tag { name: TagName, payload: Option<BoonValue> },
    Record(BTreeMap<FieldName, BoonValue>),
}

pub enum GeneratedSourceEvent {
    Static {
        source_id: SourceId,
        payload: GeneratedSourceEventPayload,
    },
    Dynamic {
        family_id: SourceFamilyId,
        owner_key: OwnerKey,
        generation: u32,
        payload: GeneratedSourceEventPayload,
    },
}
```

Example ergonomic variants must follow the same naming rule as `SourceId` and include generation for dynamic sources:

```rust
pub enum GeneratedSourceEventVariant {
    IncrementButtonEventPress,
    NewTodoInputText { text: String },
    NewTodoInputEventKeyDownKey { key: KeyTag },
    TodoCheckboxEventClick { owner: OwnerKey, generation: u32 },
    TodoRemoveButtonEventPress { owner: OwnerKey, generation: u32 },
    TodoEditInputText { owner: OwnerKey, generation: u32, text: String },
    TodoEditInputEventKeyDownKey { owner: OwnerKey, generation: u32, key: KeyTag },
    TodoEditInputEventBlur { owner: OwnerKey, generation: u32 },
}
```

These variants only inject into Timely/DD source input handles. They do not schedule Boon logic.

Compiler errors:

- Source leaf unbound by a host function.
- Source leaf bound by more than one producer.
- Source leaf read by logic but never produced.
- Source leaf shape conflicts.
- Dynamic source family without a stable owner key.
- Event for removed dynamic item with stale generation.

---

## 5. Generated Rust shape

The generated output for each example should be a Rust crate/module containing:

```text
generated/<example>/
  Cargo.toml
  src/
    lib.rs
    graph.rs
    ids.rs
    source_events.rs
    shapes.rs
    values.rs
    render_bindings.rs
    monitor_bindings.rs
    persist_bindings.rs
```

### 5.1 `ids.rs`

Generated stable IDs:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum NodeId {
    Store,
    Counter,
    CounterHold,
    IncrementButtonPressThen,
    DocumentNew,
    RootElement,
    // ...
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SourceId {
    IncrementButtonEventPress,
    IncrementButtonHovered,
    // ...
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum StorageKey {
    CounterHoldState,
    TodoCompletedState,
    // ...
}
```

### 5.2 `source_events.rs`

Generated host event enum:

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GeneratedSourceEvent {
    IncrementButtonEventPress,
    IncrementButtonHovered { value: BoolTag },
    NewTodoInputText { text: String },
    NewTodoInputEventKeyDownKey { key: KeyTag },
    TodoCheckboxEventClick { owner: OwnerKey, generation: u32 },
    TodoEditInputText { owner: OwnerKey, generation: u32, text: String },
}
```

### 5.3 `graph.rs`

Generated graph builder only:

```rust
pub struct GeneratedGraphHandles {
    pub sources: GeneratedSourceInputs,
    pub probe: timely::dataflow::operators::probe::Handle<BoonTime>,
}

pub fn build(scope: &mut Scope<BoonTime>) -> GeneratedGraphHandles {
    // construct static graph
}
```

No manually propagated dirty sets.

### 5.4 Host adapter

The host adapter owns input handles and a worker:

```rust
pub struct AppHost {
    epoch: u64,
    worker: Worker<...>,
    graph: GeneratedGraphHandles,
}

impl AppHost {
    pub fn submit_and_drain(&mut self, event: GeneratedSourceEvent, max_steps: usize) -> Result<(), DrainError> {
        self.epoch += 1;
        let target = BoonTime { epoch: self.epoch, phase: 3 };
        self.graph.sources.inject(event, BoonTime { epoch: self.epoch, phase: 0 });
        self.graph.sources.advance_to(BoonTime { epoch: self.epoch + 1, phase: 0 });
        self.graph.sources.flush();
        let mut steps = 0;
        while self.graph.probe.less_than(&target) {
            if steps == max_steps {
                return Err(DrainError::Stalled { target, steps });
            }
            self.worker.step();
            steps += 1;
        }
        Ok(())
    }
}
```

This host loop is not a Boon scheduler. It only drives Timely progress. Browser hosts must expose a bounded or yielding drain API so a stalled probe cannot spin forever or monopolize the browser UI thread. The Firefox gate must prove source injection returns control while still producing the expected monitor/render command.

---

## 6. Core operator lowering

### 6.1 Literals

A literal becomes an initial singleton collection/cell or a constant function argument.

```boon
42
TEXT { hello }
True
[]
```

For a static literal cell:

```rust
boon_dd::cell_const(NodeId::Literal42, owner, 42)
```

### 6.2 Field reads

```boon
store.sources.input.text
```

Lower to a projection over a record flow or direct source leaf collection.

### 6.3 Function calls

Pure function:

```boon
Text/trim(value)
```

Lower to `map`/`map_cell` with a generated call to the library Rust function.

Stateful function/operator:

```boon
Math/sum()
Bool/toggle(when: source)
Stream/skip(count: 1)
Keyboard/state(...)
```

Lower to a library-provided DD/Timely operator registered via metadata. The compiler supplies `NodeId` and input flows.

Effect function:

```boon
Router/go_to(path)
File/write_text(path: ..., text: ...)
Log/info(...)
```

Lower to a command flow, then effect sink. Effects execute in timestamp order after the graph has produced command records.

### 6.4 Pipe

```boon
x |> F(y: z)
```

Lower as ordinary function call where `x` becomes the implicit first argument.

### 6.5 `THEN`

```boon
source |> THEN { expr }
```

If `expr` does not read cells, use `map`.

If `expr` reads cells, use a generic `sample`/join-by-owner operator:

```text
source event at owner K + current Cell<K, V> -> event result
```

### 6.6 `WHEN`

```boon
x |> WHEN {
    Enter => expr
    __ => SKIP
}
```

For event-like input, lower to filter-map over events.

For cell-like input, lower to cell-change flow.

`SKIP` means no output record.

### 6.7 `WHILE`

```boon
selected_filter |> WHILE {
    All => True
    Active => item.completed |> Bool/not()
    Completed => item.completed
}
```

Lower to continuous switch:

- Active branch output is inserted.
- When the selector changes, old branch output is retracted.
- While branch is selected, dependencies inside that branch remain live.

Implement as generic DD operator first using selector/value collections and `map`/`join`/`concat`/`negate` as needed. If this becomes hard, implement one generic Timely operator `while_switch` with clear tests. Do not implement app-specific switches.

### 6.8 `LATEST`

```boon
LATEST {
    a
    b
    c
}
```

Lower to merge by owner key and timestamp.

Tie-breaking rule:

```text
same owner + same epoch + same phase -> highest branch index wins
```

Implement by attaching branch index to records and reducing to the max branch index for the timestamp.

### 6.9 `HOLD`

```boon
initial |> HOLD state {
    updates
}
```

Use a generic Timely operator.

Semantic rule:

- Initial state comes from persisted value if available; otherwise from `initial`.
- Update body reads the previous state value.
- Update body emits zero or one new state value per owner and source event.
- New state is emitted as a `Cell<K, State>` diff: retract old, insert new.

API sketch:

```rust
pub fn hold<S, K, State, Event, F>(
    node: NodeId,
    initial: Cell<S, K, State>,
    updates: Event<S, (K, Event)>,
    update: F,
) -> Cell<S, K, State>
where
    S: Scope<Timestamp = BoonTime>,
    K: Data + Ord + Hash,
    State: Data + Eq,
    Event: Data,
    F: Fn(&State, Event) -> Option<State> + 'static;
```

This is a Boon semantic operator, not a scheduler.

### 6.10 Lists

List operations must be keyed.

Representation:

```rust
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ListKey {
    pub stable_id: u64,
    pub generation: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ListOrder(pub u64);

pub struct ListItem<K, V> {
    pub key: K,
    pub order: ListOrder,
    pub value: V,
}
```

Lower:

```boon
LIST { a b c }
```

To initial collection:

```text
+ (key0, order0, a)
+ (key1, order1, b)
+ (key2, order2, c)
```

Lower:

```boon
list |> List/append(item: item_event)
```

To generic append operator:

```text
item_event -> allocate stable key -> insert list item
```

Allocation is deterministic and inside a generic `key_allocator` operator. For persistence, key allocator state is persistable.

Lower:

```boon
list |> List/remove(item, on: item.sources.remove_button.event.press)
```

To keyed source family + remove operator:

```text
TodoRemovePress(todo_key) -> retract item todo_key
```

Lower:

```boon
list |> List/map(item, new: todo_row(item))
```

To keyed map. The graph contains a row template, not a dynamic graph node per item.

Lower:

```boon
list |> List/retain(item, if: predicate)
```

To keyed filter. Predicate dependencies include item fields and any external cells.

### 6.11 Render sinks

The graph emits typed render commands:

```rust
pub enum RenderCommand {
    ReplaceRoot(RenderNode),
    PatchText { node: RenderNodeId, text: String },
    PatchStyle { node: RenderNodeId, style: StylePatch },
    KeyedListInsert { list: RenderNodeId, key: RenderKey, index: usize, node: RenderNode },
    KeyedListRemove { list: RenderNodeId, key: RenderKey },
    KeyedListUpdate { list: RenderNodeId, key: RenderKey, patch: RenderPatch },
}
```

Ratatui can redraw from a render tree cache. Native/browser WebGPU can update scene/UI buffers. The graph should still emit patch commands for monitoring and deterministic testing.

Render/input binding schema:

```rust
pub struct RenderNode {
    pub id: RenderNodeId,
    pub owner: OwnerKey,
    pub kind: RenderNodeKind,
    pub source_bindings: Vec<RenderSourceBinding>,
    pub children: Vec<RenderNode>,
}

pub struct RenderSourceBinding {
    pub source_id: Option<SourceId>,
    pub family_id: Option<SourceFamilyId>,
    pub owner_key: OwnerKey,
    pub generation: u32,
    pub host_event: HostEventKind,
    pub payload_shape: Shape,
}
```

Hosts convert platform input to `GeneratedSourceEvent` only through these render bindings and generated source metadata. Runtime discovery of source kinds from rendered elements is forbidden. Initial `ReplaceRoot` must precede patches for the same root epoch, and keyed list patches must be ordered by `(epoch, phase, command_order)`.

### 6.12 Effect sinks

Effects become command streams:

```rust
pub enum EffectCommand {
    RouterGoTo { path: String },
    FileWriteText { path: String, text: String },
    LogInfo { text: String },
    BuildSucceed,
    BuildFail { reason: String },
}
```

The effect runner executes commands in deterministic `(epoch, phase, command_order)` order.

---

## 7. Monitoring

Every variable, function call, operator, field read, literal, source, and effect receives a `NodeId`.

The compiler emits a static graph file:

```text
generated/<example>/graph_static.json
```

Shape:

```json
{
  "nodes": [
    {
      "id": "CounterHold",
      "kind": "Hold",
      "shape": "Number(Int)",
      "source_span": "examples/counter_hold/source.bn:3:5-7:6",
      "owner_template": "Root"
    }
  ],
  "edges": [
    { "from": "IncrementPressThen", "to": "CounterHold" }
  ]
}
```

At runtime, monitor taps emit:

```rust
pub enum MonitorRecord {
    NodeValue {
        epoch: u64,
        node: NodeId,
        owner: OwnerKey,
        value_hash: u64,
        value_preview: ValuePreview,
    },
    NodeDiff {
        epoch: u64,
        node: NodeId,
        inserted: u64,
        removed: u64,
        updated: u64,
        active_records: u64,
    },
    NodeTiming {
        epoch: u64,
        node: NodeId,
        duration_ns: u64,
    },
    EffectRequested {
        epoch: u64,
        node: NodeId,
        preview: ValuePreview,
    },
    PersistWrite {
        epoch: u64,
        node: NodeId,
        key: StorageKey,
    },
}
```

If exact per-node timing inside DD operators is not trivial, start with:

- input/output diff counts,
- value previews,
- active collection sizes,
- transaction duration,
- Timely operator names mapped to Boon `NodeId`.

Do not block examples on perfect timing. But every graph node must be inspectable and have a stable `NodeId`.

### 7.1 Realtime graph render

Every playground must include a monitor panel/mode:

- Terminal: split Ratatui screen: app + graph list + selected node details.
- Native window: overlay/debug side panel drawn by WebGPU UI renderer.
- Browser window: DOM or WebGPU overlay side panel.

Minimal monitor UI:

```text
Nodes changed this epoch:
  CounterHold       value: 12
  DocumentText      patch: "12"
  PersistCounter    wrote: yes

Selected node:
  id: CounterHold
  kind: HOLD
  shape: Number(Int)
  owner: Root
  last value: 12
  source span: examples/counter_hold/source.bn:...
```

For list templates, render instance summaries:

```text
TodoRow template
  active instances: 1000
  changed this epoch: 1
  inspect instance: TodoId(42)
```

Do not expand 1000 row graphs by default.

---

## 8. Persistence

Persistence should be graph-native, not scheduler-native.

Three independent toggles:

1. **Semantic state persistence**: `HOLD`, stateful library actors, source cells if marked persistable, list key allocators.
2. **Monitor snapshot persistence**: last value preview/hash/diff count/timing for graph debug UI.
3. **Event log persistence/replay**: source events recorded for deterministic replay.

Materialized cache persistence is optional later.

### 8.1 Storage backend

Start with JSONL or SQLite. JSONL is enough for milestone 1.

Suggested files:

```text
.boon_state/<example>/semantic_state.json
.boon_state/<example>/monitor_snapshot.json
.boon_state/<example>/event_log.jsonl
```

Storage records:

```rust
pub struct PersistedCellRecord {
    pub storage_key: StorageKey,
    pub owner: OwnerKey,
    pub value_shape_hash: u64,
    pub value_json: serde_json::Value,
}
```

On startup:

- Load persisted state records.
- Inject them at `BoonTime { epoch: 0, phase: 0 }`.
- If no persisted value exists, inject source-code initial value.
- If shape hash mismatches, either use a generated migration if available or reject with a clear error.

### 8.2 Persistence annotations

Initial mode: playground toggle controls all persistable state.

Later: add Boon annotations only if needed. Do not add syntax in milestone 1.

---

## 9. Library metadata

The compiler should read Rust-side metadata for host/library functions.

Use Rust declarations/macros or generated schema data. Keep it simple.

Example:

```rust
pub struct FunctionSchema {
    pub boon_name: &'static str,
    pub purity: Purity,
    pub inputs: &'static [ParamSchema],
    pub output: ShapeSchema,
    pub source_bindings: &'static [SourceBindingSchema],
    pub render_kind: Option<RenderKind>,
    pub effect_kind: Option<EffectKind>,
    pub dd_lowering: Option<DdLoweringKind>,
}
```

Purity:

```rust
pub enum Purity {
    Pure,
    StatefulActor,
    SourceProducer,
    EffectCommand,
    RenderConstructor,
}
```

Examples:

```rust
Element/button:
  purity: RenderConstructor
  source_bindings:
    element.event.press -> EmptyRecord
    element.hovered -> BoolTag

Timer/interval:
  purity: SourceProducer
  output: EmptyRecord event stream

Window/animation_frame:
  purity: SourceProducer
  output: Record { delta: Number, now: Number }

Math/sum:
  purity: StatefulActor
  dd_lowering: HoldLikeAccumulator

Text/trim:
  purity: Pure

Router/go_to:
  purity: EffectCommand
```

No app-specific metadata.

---

## 10. Example rewrites / expected source shape

### 10.1 Counter

```boon
store: [
    sources: [
        increment_button: [
            event: [
                press: SOURCE
            ]
        ]
    ]

    counter:
        store.sources.increment_button.event.press
        |> THEN { 1 }
        |> Math/sum()
]

document:
    Document/new(
        root:
            Element/button(
                element: store.sources.increment_button
                style: []
                label: store.counter |> Text/from_number()
            )
    )
```

Acceptance:

- Initial label `0` or equivalent expected counter start.
- Each press increments by 1.
- Monitor shows source press, `THEN`, `Math/sum`, document/render patch.
- Persistence toggle restores counter after restart if enabled.

### 10.2 Counter with HOLD

```boon
store: [
    sources: [
        increment_button: [event: [press: SOURCE]]
    ]

    counter:
        0 |> HOLD state {
            store.sources.increment_button.event.press
            |> THEN { state + 1 }
        }
]
```

Acceptance:

- Same UX as counter.
- Monitor shows `HOLD` state transition.
- Persistence restores the held state.

### 10.3 Interval

```boon
tick:
    Duration[seconds: 1] |> Timer/interval()

counter:
    tick |> THEN { 1 } |> Math/sum()

document:
    counter |> Document/new()
```

Acceptance:

- Counter increments once per second.
- Test harness can inject synthetic ticks without wall-clock delay.
- Monitor shows timer source.

### 10.4 Interval with HOLD

```boon
tick:
    Duration[seconds: 1] |> Timer/interval()

counter:
    0 |> HOLD state {
        tick |> THEN { state + 1 }
    }
    |> Stream/skip(count: 1)

document:
    counter |> Document/new()
```

Acceptance:

- First tick can be skipped as source example expects.
- Persisted state resumes correctly.

### 10.5 Pong

Pong must be ordinary Boon + library functions:

```boon
store: [
    sources: [
        frame: Window/animation_frame()
        keyboard: [
            event: [
                key_down: [key: SOURCE]
                key_up: [key: SOURCE]
            ]
        ]
    ]

    keys:
        Keyboard/state(
            down: sources.keyboard.event.key_down.key
            up: sources.keyboard.event.key_up.key
        )

    game:
        Pong/initial() |> HOLD game {
            sources.frame |> THEN {
                Pong/step(
                    game: game
                    keys: keys
                    dt: sources.frame.delta
                )
            }
        }
]

scene:
    Pong/view(game: store.game)
```

Acceptance:

- No Pong-specific transpiler code.
- `Pong/step` is pure library/user function.
- `Keyboard/state` is a stateful library DD operator.
- `Window/animation_frame` is a source producer.
- Monitor can inspect `keys`, `game`, `Pong/step`, and scene patches.

### 10.6 TodoMVC

Important source rules:

- Todo input text is `store.sources.new_todo_input.text`.
- Enter detection is `store.sources.new_todo_input.event.key_down.key`.
- KeyDown does not carry text.
- Todo row sources become dynamic source families under stable todo keys.

Sketch:

```boon
store: [
    sources: [
        new_todo_input: [
            text: SOURCE
            event: [
                key_down: [key: SOURCE]
                focus: SOURCE
                blur: SOURCE
            ]
        ]
        toggle_all_checkbox: [event: [click: SOURCE]]
        clear_completed_button: [event: [press: SOURCE]]
        filter_all_button: [event: [press: SOURCE], hovered: SOURCE]
        filter_active_button: [event: [press: SOURCE], hovered: SOURCE]
        filter_completed_button: [event: [press: SOURCE], hovered: SOURCE]
    ]

    new_todo_text:
        Text/empty() |> HOLD text {
            LATEST {
                sources.new_todo_input.text
                title_to_add |> THEN { Text/empty() }
            }
        }

    title_to_add:
        sources.new_todo_input.event.key_down.key |> WHEN {
            Enter => BLOCK {
                title: new_todo_text |> Text/trim()
                title |> Text/is_not_empty() |> WHEN {
                    True => title
                    False => SKIP
                }
            }
            __ => SKIP
        }

    todos:
        LIST {
            new_todo(title: TEXT { Buy groceries })
            new_todo(title: TEXT { Clean room })
        }
        |> List/append(item: title_to_add |> new_todo())
        |> List/remove(item, on: item.sources.remove_button.event.press)
        |> List/remove(item, on: sources.clear_completed_button.event.press |> THEN {
            item.completed |> WHEN {
                True => []
                False => SKIP
            }
        })
]
```

Acceptance:

- Visual look remains compatible with existing TodoMVC example.
- Single checkbox toggle changes one todo row and counts.
- Toggle All changes all todo completed states.
- Clear completed removes completed todos.
- Filter buttons update visible list.
- Monitor shows keyed source families and list template instances.
- Persistence stores todos, titles, completed state, edit state if enabled.

### 10.7 TodoMVC Physical

TodoMVC Physical should use the same logical `todo_mvc` source/state graph plus physical render library calls:

```boon
scene:
    Scene/new(
        root: root_element(store: store)
        lights: Theme/lights(theme: store.theme)
        camera: Theme/camera(theme: store.theme)
    )
```

Acceptance:

- No physical UI logic in transpiler.
- `Scene/Element/*`, `Theme/*`, `Material/*` are render constructors / pure functions.
- Native and browser WebGPU show the same scene semantics.
- Terminal playground must provide an explicit terminal renderer for the physical UI semantics. This is a planned renderer target, not a fallback path; if the semantic terminal renderer is not implemented, the terminal physical-UI test fails.
- Monitor shows render tree and source/state nodes.

---

## 11. Playgrounds

### 11.1 Shared playground model

Each playground host receives:

```rust
pub trait PlaygroundHost {
    fn load_example(&mut self, example: ExampleName);
    fn submit_source_event(&mut self, event: GeneratedSourceEvent);
    fn receive_render_commands(&mut self) -> Vec<RenderCommand>;
    fn receive_monitor_records(&mut self) -> Vec<MonitorRecord>;
    fn set_persistence_enabled(&mut self, enabled: bool);
    fn set_monitor_enabled(&mut self, enabled: bool);
}
```

The host does not run Boon logic. It only:

- injects source events,
- steps the Timely worker/probe,
- applies render/effect/persist/monitor command outputs,
- handles platform-specific input/output.

### 11.2 Terminal playground

Target command:

```bash
cargo xtask run --example counter --target terminal
cargo xtask run --example todo_mvc --target terminal
cargo xtask test --target terminal
```

Features:

- Ratatui app panel.
- Monitor panel.
- Source/event log panel.
- Keyboard shortcuts:
  - `Tab`: switch focus between app and monitor.
  - `p`: toggle semantic persistence.
  - `m`: toggle monitor overlay.
  - `r`: reset example state.
  - `q`: quit.

Acceptance tests:

- Golden terminal snapshots.
- Scenario scripts inject source events directly.
- Ratatui diff rendering is host-level only; Boon graph still emits deterministic patches.

### 11.3 Native window playground

Target command:

```bash
cosmic-background-launch --workspace boon-dd -- cargo xtask run --example counter --target native
cosmic-background-launch --workspace boon-dd -- cargo xtask run --example todo_mvc_physical --target native
cargo xtask test --target native
```

Use:

- `app_window` for cross-platform window/input.
- `wgpu` for rendering.
- WESL/WGSL shader sources.
- `wgsl_bindgen` or a simple binding layer for shader interface generation.

Features:

- App view.
- Optional graph monitor overlay.
- Optional pixel/framebuffer readback for tests.
- Same generated graph as terminal target.

Acceptance tests:

- Counter: click/increment, framebuffer or render-command assertion.
- TodoMVC: add/toggle/filter/clear, render-command assertion and optional readback.
- Pong: deterministic synthetic frame events, game state/render patch assertions.
- TodoMVC Physical: smoke test scene renders and responds to source events.
- Any test that opens a native window must be launched through `cosmic-background-launch --workspace boon-dd -- ...`; foreground native windows are a verification failure.

### 11.4 Browser window playground

Target commands:

```bash
cosmic-background-launch --workspace boon-dd -- cargo xtask run --example counter --target browser
cosmic-background-launch --workspace boon-dd -- cargo xtask run --example todo_mvc_physical --target browser
cargo xtask test --target browser --browser firefox
```

Only one implementation mode is allowed:

#### Browser-hosted graph

- Compile the generated graph + Boon DD kernel + required Timely/Differential dependencies to `wasm32-unknown-unknown`.
- Run the Timely/Differential graph inside the browser-hosted WASM module.
- Use browser WebGPU/wgpu for rendering.
- Host input events inject directly into WASM graph handles.
- Render, effect, persistence, and monitor commands are emitted by the same in-browser Timely/Differential graph.

Forbidden browser modes:

- No native graph worker.
- No WebSocket/WebTransport bridge for Boon semantics.
- No custom JS/browser scheduler for Boon semantics.
- No interpretation fallback.
- No partial browser target that bypasses Timely/Differential.

Acceptance tests:

- Firefox-first test harness.
- Scenario scripts inject events and assert monitor/render command outputs.
- Local verification must run a minimal browser/WASM graph smoke test before browser renderer work is accepted.
- If `timely` or `differential-dataflow` fails to compile or run in browser-hosted WASM, stop the browser milestone and fix/pin/fork the dependency. Do not build a bridge fallback.
- Any test that opens Firefox or another browser window must be launched through `cosmic-background-launch --workspace boon-dd -- ...`; foreground browser windows are a verification failure.

---

## 12. Verification and testing

### 12.0 Local verification gates

All gates are local. Do not require remote automation to prove this plan.

Required top-level gate:

```bash
cargo xtask verify all --format json
```

It must run or dispatch:

- `cargo xtask bootstrap --check`
- `cargo xtask verify-deps --format json`
- `cargo xtask verify-wasm-dd --required --browser firefox`
- compiler tests
- DD kernel tests, including native and browser-WASM coverage for every kernel operator when introduced
- scenario tests for the canonical example matrix
- forbidden-pattern scan for dirty scheduler APIs
- focus-safe launch preflight for any native/browser GUI tests

`verify-report.json` must include each gate name, command, status, duration, relevant artifact paths, and blocker path if failed. `success.json` must be written only when every required local gate passes.

### 12.1 Scenario files

Use example-local scenario files:

```text
examples/counter/scenario.toml
examples/todo_mvc/scenario.toml
```

Shape:

```toml
[initial]
expect_text = "0"

[[step]]
description = "increment"
actions = [
  { source = "store.sources.increment_button.event.press", value = [] }
]
expect_text = "1"
expect_monitor_changed = ["IncrementButtonPress", "Counter", "Document"]

[[step]]
description = "persist and reload"
actions = [
  { command = "enable_persistence" },
  { source = "store.sources.increment_button.event.press", value = [] },
  { command = "reload" }
]
expect_text = "2"
```

Tests should inject source events directly at the generated source boundary rather than depending on UI clicks. UI click tests are additional host tests.

### 12.2 Compiler tests

Add tests for:

- Parser support for records, text, tags, `SOURCE`, `HOLD`, `LATEST`, `WHEN`, `THEN`, `WHILE`, pipes.
- Shape inference for source records.
- Error on source shape conflict.
- Error on unbound source.
- Dynamic source family owner stability.
- KeyDown has key only, no text.
- Text input text comes from `.text`.

### 12.3 DD kernel tests

Add direct Rust tests for:

- `then_const`
- `when`
- `latest` tie-break
- `hold`
- `keyed_hold`
- `while_switch`
- `list_append`
- `list_remove`
- `list_map`
- `list_retain`
- source injection and probe completion
- monitor tap
- persist tap

### 12.4 End-to-end tests

Required matrix:

```text
example                    terminal required  native required  browser required
counter                    1                  5                7
counter_hold               1                  5                7
interval                   2                  5                7
interval_hold              2                  5                7
latest                     2                  5                7
when                       2                  5                7
while                      2                  5                7
then                       2                  5                7
list_map_block             3                  5                7
list_map_external_dep      3                  5                7
list_object_state          3                  5                7
list_retain_count          3                  5                7
list_retain_reactive       3                  5                7
list_retain_remove         3                  5                7
shopping_list              3                  5                7
todo_mvc                   4                  5                7
crud                       4                  5                7
flight_booker              4                  5                7
temperature_converter      4                  5                7
pong                       6                  6                7
cells                      6                  6                7
todo_mvc_physical          8                  8                8
```

Browser matrix entries are pass/fail only. Do not mark browser-hosted graph as pending because of a bridge fallback.

Each matrix row must have scenario fixtures with expected render/monitor JSON. Terminal snapshots use a fixed `120x40` viewport. Native/browser render verification uses a fixed `1280x720` viewport at DPR `1.0` when pixel/framebuffer checks are required. Subjective acceptance words such as “works” are not sufficient gate criteria.

---

## 13. Milestones

### Milestone 0: Repo reset, dependency lock, and architecture guardrails

Tasks:

- Keep this root plan as the canonical source of truth. If a docs copy is needed later, make it a pointer to this file instead of a divergent copy.
- Add `ARCHITECTURE.md` summarizing DD-first rule.
- Add a local `xtask` grep gate that fails if new code adds obvious dirty scheduler APIs like `mark_dirty`, `dirty_nodes`, `recompute_dependents`, except inside comments/tests documenting forbidden patterns.
- Pin dependencies, features, Rust toolchain, wasm tooling, and renderer tooling.
- Create `boon_dd` crate/module.
- Add `cargo xtask bootstrap --check`, `cargo xtask verify-deps --format json`, and `cargo xtask verify all --format json`.
- Add focus-safe GUI preflight for `cosmic-background-launch`.

Acceptance:

- Workspace builds locally.
- `cargo xtask bootstrap --check` passes.
- `cargo xtask verify-deps --format json` passes and records exact versions/features/tool paths.
- Plan is checked in as the root source of truth.
- No custom dirty scheduler is added.
- `cosmic-background-launch` and its user D-Bus service are verified before native/browser GUI gates run.

### Gate 0.5: Browser-hosted Timely/DD proof

Tasks:

- Create a tiny generated graph crate that uses Timely + Differential Dataflow with default features disabled where required.
- Prove `BoonTime` or the chosen encoded timestamp in a real Timely/DD graph with `map`, `join` or `reduce`, `probe`, `then_const`, and `hold`.
- Compile it to `wasm32-unknown-unknown`.
- Run a Firefox smoke test through `cosmic-background-launch --workspace boon-dd -- ...` that inserts one input record, advances/probes the graph with bounded drain, and observes one expected monitor/render output diff.
- Create a minimal hand-written Boon DD graph using `timely` + `differential-dataflow`: one source, one `then_const`, one `hold`, one monitor output.
- Keep all default features disabled where needed.

Acceptance:

- `cargo xtask verify-wasm-dd --required --browser firefox` passes.
- `cargo check --target wasm32-unknown-unknown -p boon_dd` passes.
- The minimal generated-style graph builds to WASM.
- Firefox receives the expected monitor/render command and returns control without an unbounded spin.
- If this cannot be achieved, the full `/goal` stops with `docs/blockers/timely-dd-wasm.md`. No fallback implementation is allowed.

### Milestone 1: Minimal graph kernel + counter

Tasks:

- Implement source injection.
- Implement `Event`, `Cell`, `Command` wrappers.
- Implement `then_const`, `hold`, `monitor_tap`, `render_text`.
- Generate graph for `counter` and `counter_hold`.
- Terminal playground only.
- Add scenario fixtures and expected render/monitor JSON for `counter` and `counter_hold`.

Acceptance:

- `cargo xtask run --example counter --target terminal` renders the expected initial text and exits cleanly under scripted input.
- `counter_hold` scripted press events increment according to fixture expectations.
- Monitor output contains the expected source, `THEN`, `HOLD`, render, and persistence records.
- Persistence toggle stores/restores `counter_hold` according to fixture JSON.

### Milestone 2: Timer + LATEST/WHEN/THEN/WHILE basics

Tasks:

- Implement timer source producer.
- Implement `when`.
- Implement `latest` tie-break.
- Implement enough `while_switch` for examples.
- Implement `Stream/skip` as library operator.
- Run `interval`, `interval_hold`, `latest`, `when`, `while`, `then`.
- Add native and browser-WASM tests for each introduced DD kernel operator.

Acceptance:

- Synthetic tick tests pass without waiting wall-clock seconds.
- Monitor shows source/timer nodes.
- Scenario fixtures pass for all Milestone 2 rows on terminal. Native/browser coverage for those rows becomes required at their matrix milestones.

### Milestone 3: Keyed lists and dynamic source families

Tasks:

- Implement stable list keys.
- Implement `List/append`, `List/remove`, `List/map`, `List/retain`, `List/count`, `List/every`.
- Implement dynamic source families under list item owners.
- Implement keyed render list patches.
- Run list examples and shopping list.
- Verify dynamic source events carry `owner_key` and `generation`.

Acceptance:

- Single list item update emits one keyed patch when a key-preserving patch is valid; otherwise it emits a deterministic keyed remove/insert pair.
- Removing an item invalidates its dynamic source generation.
- Stale event for removed item is rejected/ignored with monitor record.
- Scenario fixtures pass for all Milestone 3 rows on terminal. Native/browser coverage for those rows becomes required at their matrix milestones.

### Milestone 4: TodoMVC and small apps

Tasks:

- Rewrite TodoMVC example to source-record `SOURCE` style.
- Ensure KeyDown has no text.
- Implement text input `.text` source.
- Implement filters, clear completed, toggle all, edit title.
- Run in terminal playground.
- Add scenario fixtures for TodoMVC, CRUD, Flight Booker, and Temperature Converter.

Acceptance:

- TodoMVC fixture covers add, toggle, filter, clear completed, edit title, and persistence restore.
- Monitor graph shows static graph + keyed row instances.
- Persistence stores todos.
- Release-mode 1000 synthetic todos pass recorded thresholds for single toggle and toggle-all.
- Scenario fixtures pass for all Milestone 4 rows on terminal. Native/browser coverage for those rows becomes required at their matrix milestones.

### Milestone 5: Native wgpu playground

Tasks:

- Build simple native window using `app_window`.
- Build render IR to wgpu renderer.
- Add WESL/WGSL shader pipeline.
- Run counter/counter_hold/interval/todo_mvc in native window.
- Add monitor overlay.
- Add `cargo xtask verify-render-deps --format json` for native surface smoke, WESL-to-WGSL compile, shader validation, and deterministic render-command or pixel/readback assertion.

Acceptance:

- `cargo xtask verify-render-deps --format json` passes locally.
- Native window opens only through `cosmic-background-launch --workspace boon-dd -- ...`.
- Native window loads examples from scripted launch.
- Input events route to source injection.
- Render-command JSON or framebuffer/readback artifacts match fixtures.

### Milestone 6: Pong

Tasks:

- Implement `Window/animation_frame` source.
- Implement `Keyboard/state` library operator.
- Add `Pong/initial`, `Pong/step`, `Pong/view` as library/user functions.
- Run Pong and Cells through the canonical matrix.

Acceptance:

- Synthetic frame tests are deterministic.
- No Pong-specific compiler logic.
- Monitor shows game state evolution.
- Scenario fixtures pass for Pong and Cells on terminal/native/browser.

### Milestone 7: Browser playground

Tasks:

- Build browser playground infrastructure after Gate 0.5 has passed.
- Add Firefox test harness launched through `cosmic-background-launch --workspace boon-dd -- ...`.
- Run counter, TodoMVC, Pong, and non-physical matrix rows inside browser-hosted WASM.
- Grow browser-WASM tests with each DD kernel operator; do not leave browser coverage as a single smoke graph.

Acceptance:

- Browser window exercises examples with the generated Timely/Differential graph running inside the browser WASM module.
- Source events and render/monitor patches round-trip deterministically.
- Browser target fails fast if Timely/Differential browser-hosted WASM is unavailable.
- Foreground browser windows are never opened by verification.

### Milestone 8: TodoMVC Physical

Tasks:

- Keep physical UI as render/library functions.
- Implement enough scene/render IR for physical example.
- WESL/WGSL shaders compile and are reused native/browser.
- Terminal uses the explicit terminal semantic renderer for physical UI. If that renderer is not implemented, the terminal target fails rather than substituting an ad-hoc fallback.
- Add physical scene fixtures for native/browser render-command or pixel/readback verification.

Acceptance:

- Native and browser render physical scene.
- Terminal still exercises source/state logic.
- Monitor can inspect scene nodes and app nodes.
- TodoMVC Physical passes the canonical matrix after physical renderer support lands.

### Milestone 9: Full example matrix and docs

Tasks:

- Run all important examples across the canonical matrix.
- Add docs for writing new examples.
- Add docs for source records.
- Add docs for monitor/persistence toggles.
- Add performance notes.

Acceptance:

- `cargo xtask verify all --format json` reports the full scenario matrix passing locally.
- `target/boon-artifacts/success.json` reports `success: true`.
- New user can write an app without transpiler changes.

---

## 14. Performance expectations

These are not hard test thresholds for early milestones, but they guide design.

For 1000 TodoMVC todos in release mode:

```text
single checkbox toggle:
  expected graph work: O(1) keyed source + O(1) row/count patches

toggle all:
  expected graph work: O(N) state changes and row patches unless a later default+override optimization is added
```

Do not optimize toggle-all with a custom TodoMVC special case. A generic optimization may be added later:

```text
field_default + per-key overrides
```

But milestone 4 should use normal keyed state and accept O(N) toggle-all.

---

## 15. Risks and required decisions

### 15.1 Timely/DD WebAssembly support

Current evidence as of 2026-05-03:

- Timely master contains a dedicated `wasm-bindgen` browser example and upstream automation checks `timely_communication`, `timely`, the `threadless` example, and the `wasm-bindgen` example against `wasm32-unknown-unknown`.
- Differential master has an open WebAssembly-support issue and its visible upstream automation does not include a wasm target check. Treat DD browser-WASM support as a local implementation gate, not as proven.

Fail-fast plan:

1. Before implementing browser UI, create a minimal generated-graph crate that depends on the chosen `timely` and `differential-dataflow` revisions with `default-features = false`.
2. The crate must compile for `wasm32-unknown-unknown` and run a tiny in-browser graph: one source, one DD collection/count or reduce, one probe, one monitor output.
3. Add this smoke test to local `cargo xtask verify-wasm-dd --required --browser firefox`.
4. If this fails, stop the browser milestone and fix/pin/fork Timely/DD. Do not introduce a native graph worker, JS scheduler, interpreter fallback, or non-DD browser semantics.
5. Keep terminal and native targets progressing only if they use the same graph-construction semantics and do not create a divergent browser architecture.

### 15.2 HOLD as generic Timely operator

Risk: implementing `HOLD` purely in DD algebra may be awkward.

Plan:

- Implement `hold` as a small generic Timely operator with keyed state.
- Keep it heavily tested.
- Do not spread state logic into app-generated scheduler functions.

### 15.3 LATEST tie-break

Risk: inconsistent same-epoch ordering.

Plan:

- Attach branch index.
- Use deterministic reduce.
- Add tests for same-epoch conflicts.

### 15.4 Dynamic sources and stale events

Risk: events for removed list rows.

Plan:

- Dynamic source family event includes owner key + generation.
- Runtime checks active owner generation before injecting.
- Stale events produce monitor warning and no Boon value.

### 15.5 Library metadata drift

Risk: host schema and renderer implementation disagree.

Plan:

- Keep host schemas in Rust constants/macros near the host function implementation.
- Generate tests from schemas.
- Fail compile on unknown source leaf fields.

---

## 16. Forbidden implementation patterns

Do not implement:

- Custom dirty-node propagation scheduler.
- `mark_dirty` / `recompute_dependents` as core runtime design.
- Per-edge channels for Boon graph edges.
- Async tasks/futures as Boon’s internal graph model.
- App-specific lowerings for TodoMVC, Pong, Counter, etc.
- `SOURCE(Something)` syntax.
- Key-down event with text payload.
- Full document rebuild as the primary UI update mechanism for lists.
- Dynamic Timely/DD graph creation per todo/list item.
- Runtime discovery of source kinds from rendered elements.
- Nominal user-facing source/capability types.
- Browser fallback that runs Boon semantics outside browser WASM.
- JS reimplementation of Boon semantics.

Allowed:

- Host async at boundaries: window events, browser, wgpu init/readback, file/network adapters.
- A small generic Timely operator for `HOLD`/`keyed_hold`.
- A tiny host loop that injects inputs and steps probes.
- Renderer caches outside Boon semantics.
- DD subgraphs with generated typed Rust closures.

---

## 17. First Codex CLI task list

Start here.

1. Read this file fully.
2. Add or verify local `xtask` support for `cargo xtask bootstrap --check`, `cargo xtask verify-deps --format json`, `cargo xtask verify-wasm-dd --required --browser firefox`, and `cargo xtask verify all --format json`.
3. Pin the Rust toolchain, dependencies, feature flags, `Cargo.lock`, wasm tooling, renderer tooling, and focus-safe GUI preflight.
4. Inspect the current workspace and identify code implementing custom dirty scheduling.
5. Add `boon_dd` crate/module with:
   - `BoonTime`
   - `Event`, `Cell`, `Command`
   - source input handle wrapper
   - `then_const`
   - `hold`
   - `monitor_tap`
   - simple text render command sink
6. Immediately add `wasm32-unknown-unknown` `cargo check` for `boon_dd` and a minimal hand-written graph. This is a fail-fast gate, not a late browser task.
7. Run `cargo xtask verify-wasm-dd --required --browser firefox`; if it fails, write `docs/blockers/timely-dd-wasm.md` and stop the full `/goal`.
8. Add a tiny hand-written generated graph for `counter_hold` before writing full codegen.
9. Run it in terminal with source injection test and fixture assertions.
10. Only after the hand-written DD graph passes native and browser-WASM gates, implement codegen to produce equivalent graph construction code.
11. Add scenario tests for `counter` and `counter_hold`.
12. Add persistence tap for `HOLD` state.
13. Implement `interval` with synthetic timer injection.
14. Implement `LATEST` and `WHEN` tests.

The first milestone gate passes when generated static Timely/DD graphs run `counter` and `counter_hold` in the terminal, expected monitor records update, persistence restore matches fixture JSON, and the relevant local `xtask` gates report success.

---

## 18. External references to keep in mind

These are not requirements to copy architecture from, only grounding references.

- Boon main repo examples and language direction: `https://github.com/BoonLang/boon`
- Existing `boon-rust` repo layout, app_window vendor, shaders, examples: `https://github.com/BoonLang/boon-rust`
- Differential Dataflow: `https://github.com/TimelyDataflow/differential-dataflow`
- Timely wasm example: `https://github.com/TimelyDataflow/timely-dataflow/tree/master/timely/examples/wasm-bindgen`
- Timely/Differential mdBook: `https://timelydataflow.github.io/differential-dataflow/`
- Verified progress tracking for Timely Dataflow: `https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.ITP.2021.10`
- Ratatui rendering model: `https://ratatui.rs/concepts/rendering/under-the-hood/`
- WESL: `https://wesl-lang.dev/`
- wgpu: `https://github.com/gfx-rs/wgpu`
