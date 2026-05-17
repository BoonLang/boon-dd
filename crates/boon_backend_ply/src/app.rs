use boon_dd::{ScenarioStep, SmokeOutput, SourceAction};
use ply_engine::math::Dimensions;
use ply_engine::prelude::*;
use ply_engine::render_commands::RenderCommandConfig;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

const SIDEBAR_WIDTH: f32 = 286.0;
const VISIBLE_ROWS: usize = 16;

#[derive(Clone)]
pub struct PlaygroundExample {
    pub name: String,
    pub scenario_steps: Vec<ScenarioStep>,
    pub runtime: LiveExampleRuntime,
    pub step_index: usize,
    pub next_epoch: u64,
    pub output: SmokeOutput,
    pub checked_scenario_output: SmokeOutput,
    pub last_auto_tick: f64,
}

#[derive(Clone)]
pub struct LiveExampleRuntime {
    inner: Rc<RefCell<dyn LiveExampleRuntimeImpl>>,
}

trait LiveExampleRuntimeImpl {
    fn submit_actions_and_drain(
        &mut self,
        actions: &[SourceAction],
        epoch: u64,
    ) -> Result<SmokeOutput, String>;
    fn graph_builds(&self) -> usize;
    fn drained_outputs(&self) -> usize;
}

struct GeneratedRuntimeAdapter<S> {
    session: S,
    submit_actions: fn(&mut S, &[SourceAction], u64) -> Result<SmokeOutput, String>,
    graph_builds: fn(&S) -> usize,
    drained_outputs: fn(&S) -> usize,
}

impl<S> LiveExampleRuntimeImpl for GeneratedRuntimeAdapter<S> {
    fn submit_actions_and_drain(
        &mut self,
        actions: &[SourceAction],
        epoch: u64,
    ) -> Result<SmokeOutput, String> {
        (self.submit_actions)(&mut self.session, actions, epoch)
    }

    fn graph_builds(&self) -> usize {
        (self.graph_builds)(&self.session)
    }

    fn drained_outputs(&self) -> usize {
        (self.drained_outputs)(&self.session)
    }
}

impl LiveExampleRuntime {
    fn generated<S: 'static>(
        session: S,
        submit_actions: fn(&mut S, &[SourceAction], u64) -> Result<SmokeOutput, String>,
        graph_builds: fn(&S) -> usize,
        drained_outputs: fn(&S) -> usize,
    ) -> Self {
        Self {
            inner: Rc::new(RefCell::new(GeneratedRuntimeAdapter {
                session,
                submit_actions,
                graph_builds,
                drained_outputs,
            })),
        }
    }

    fn submit_actions_and_drain(
        &self,
        actions: &[SourceAction],
        epoch: u64,
    ) -> Result<SmokeOutput, String> {
        self.inner
            .borrow_mut()
            .submit_actions_and_drain(actions, epoch)
    }

    pub fn graph_builds(&self) -> usize {
        self.inner.borrow().graph_builds()
    }

    pub fn drained_outputs(&self) -> usize {
        self.inner.borrow().drained_outputs()
    }
}

#[derive(Clone)]
pub struct PlaygroundState {
    pub examples: Vec<PlaygroundExample>,
    pub selected: usize,
    pub input_buffer: String,
    pub last_submitted_text: String,
    pub interaction_log: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FrameEvidence {
    pub selected_example: String,
    pub render_output_text: String,
    pub ply_render_commands: usize,
    pub rectangles: usize,
    pub texts: usize,
    pub borders: usize,
    pub unique_ids: usize,
    pub semantic_widgets: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InteractionEvidence {
    pub selected_initial: String,
    pub selected_after_next: String,
    pub counter_initial: String,
    pub counter_after_increment: String,
    pub counter_after_second_increment: String,
    pub generic_initial: String,
    pub generic_after_activation: String,
    pub selection_changed: bool,
    pub counter_state_changed: bool,
    pub counter_advanced_twice: bool,
    pub generic_state_changed_or_noop_documented: bool,
}

impl PlaygroundState {
    pub fn new() -> Self {
        Self {
            examples: build_playground_examples(),
            selected: 0,
            input_buffer: String::new(),
            last_submitted_text: String::new(),
            interaction_log: Vec::new(),
        }
    }

    pub fn select_by_name(&mut self, name: &str) {
        if let Some(index) = self
            .examples
            .iter()
            .position(|example| example.name == name)
        {
            self.select_index(index);
        }
    }

    pub fn select_index(&mut self, index: usize) {
        let next = index.min(self.examples.len().saturating_sub(1));
        if next != self.selected {
            self.selected = next;
            self.interaction_log
                .push(format!("selected:{}", self.selected_name()));
        }
    }

    pub fn selected_name(&self) -> &str {
        self.examples
            .get(self.selected)
            .map(|example| example.name.as_str())
            .unwrap_or("")
    }

    pub fn selected_output_text(&self) -> String {
        self.examples
            .get(self.selected)
            .map(|example| render_text(&example.output))
            .unwrap_or_default()
    }

    pub fn selected_action_count(&self) -> usize {
        self.examples
            .get(self.selected)
            .map(|example| example.step_index)
            .unwrap_or(0)
    }

    pub fn selected_monitor_count(&self) -> usize {
        self.examples
            .get(self.selected)
            .map(|example| example.output.monitor.len())
            .unwrap_or(0)
    }

    pub fn type_text(&mut self, text: &str) {
        self.input_buffer.push_str(text);
        self.interaction_log.push(format!("typed_text:{text}"));
    }

    pub fn press_key_label(&mut self, key: &str) {
        match key {
            "Home" => self.select_first(),
            "End" => self.select_last(),
            "ArrowDown" | "Down" => self.select_next(),
            "ArrowUp" | "Up" => self.select_prev(),
            "ArrowRight" | "Right" => self.select_next(),
            "ArrowLeft" | "Left" => self.select_prev(),
            "Enter" | "Space" | " " => self.submit_selected(key),
            "-" | "Minus" => self
                .interaction_log
                .push(format!("decrement_unbound:{}", self.selected_name())),
            "Backspace" => {
                self.input_buffer.pop();
                self.interaction_log.push("key:Backspace".to_owned());
            }
            other => self.interaction_log.push(format!("key:{other}")),
        }
    }

    pub fn submit_selected(&mut self, key: &str) {
        if !self.input_buffer.is_empty() {
            self.last_submitted_text = self.input_buffer.clone();
            self.input_buffer.clear();
        }
        self.interaction_log.push(format!("submit_key:{key}"));
        self.trigger_selected();
    }

    pub fn loaded_examples(&self) -> Vec<String> {
        self.examples
            .iter()
            .map(|example| example.name.clone())
            .collect()
    }

    pub fn select_next(&mut self) {
        let next = (self.selected + 1).min(self.examples.len().saturating_sub(1));
        if next != self.selected {
            self.selected = next;
            self.interaction_log
                .push(format!("selected:{}", self.selected_name()));
        }
    }

    pub fn select_first(&mut self) {
        if self.selected != 0 {
            self.selected = 0;
            self.interaction_log
                .push(format!("selected:{}", self.selected_name()));
        }
    }

    pub fn select_last(&mut self) {
        let next = self.examples.len().saturating_sub(1);
        if self.selected != next {
            self.selected = next;
            self.interaction_log
                .push(format!("selected:{}", self.selected_name()));
        }
    }

    pub fn select_prev(&mut self) {
        let next = self.selected.saturating_sub(1);
        if next != self.selected {
            self.selected = next;
            self.interaction_log
                .push(format!("selected:{}", self.selected_name()));
        }
    }

    pub fn trigger_selected(&mut self) {
        if let Some(example) = self.examples.get_mut(self.selected) {
            trigger_example_action(example);
            self.interaction_log
                .push(format!("activated:{}", example.name));
        }
    }

    pub fn counter_index(&self) -> Option<usize> {
        self.examples
            .iter()
            .position(|example| example.name == "counter")
    }

    pub fn trigger_counter_increment(&mut self) {
        if let Some(index) = self.counter_index()
            && let Some(example) = self.examples.get_mut(index)
        {
            trigger_example_action(example);
            self.interaction_log.push("counter:increment".to_owned());
        }
    }

    pub fn update_auto_tick(&mut self, now: f64) {
        let Some(example) = self.examples.get_mut(self.selected) else {
            return;
        };
        if !matches!(example.name.as_str(), "interval" | "interval_hold") {
            return;
        }
        if now - example.last_auto_tick < 1.0 {
            return;
        }
        example.last_auto_tick = now;
        trigger_example_action(example);
        self.interaction_log
            .push(format!("auto_tick:{}", example.name));
    }
}

pub fn handle_keyboard(state: &mut PlaygroundState) {
    let mut typed_space = false;
    let decrement_pressed = is_key_pressed(KeyCode::Minus) || is_key_pressed(KeyCode::KpSubtract);
    while let Some(character) = get_char_pressed() {
        if !character.is_control() && !(character == '-' && decrement_pressed) {
            typed_space |= character == ' ';
            state.type_text(&character.to_string());
        }
    }
    if is_key_pressed(KeyCode::Home) {
        state.press_key_label("Home");
    }
    if is_key_pressed(KeyCode::End) {
        state.press_key_label("End");
    }
    if is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::Right) {
        state.press_key_label("ArrowDown");
    }
    if is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::Left) {
        state.press_key_label("ArrowUp");
    }
    if is_key_pressed(KeyCode::Enter)
        || is_key_pressed(KeyCode::KpEnter)
        || (is_key_pressed(KeyCode::Space) && !typed_space)
    {
        state.press_key_label("Enter");
    }
    if is_key_pressed(KeyCode::Equal) || is_key_pressed(KeyCode::KpAdd) {
        state.press_key_label("Space");
    }
    if decrement_pressed {
        state.press_key_label("-");
    }
}

pub fn handle_pointer(state: &mut PlaygroundState, ply: &Ply<()>) -> bool {
    for index in visible_example_range(state) {
        if ply.is_just_pressed(Id::new_index("example", index as u32)) {
            state.select_index(index);
            state
                .interaction_log
                .push(format!("mouse_select:{}", state.selected_name()));
            return true;
        }
    }

    let mut handled = false;
    if ply.is_just_pressed("example-activate") {
        state.trigger_selected();
        state.interaction_log.push("mouse:activate".to_owned());
        handled = true;
    }
    handled
}

pub fn evaluate_frame(ply: &mut Ply<()>, state: &PlaygroundState) -> FrameEvidence {
    {
        let mut ui = ply.begin();
        build_ui(&mut ui, state);
    }
    let commands = ply.eval();
    frame_evidence_from_commands(state, &commands)
}

pub async fn show_frame(ply: &mut Ply<()>, state: &PlaygroundState) {
    {
        let mut ui = ply.begin();
        build_ui(&mut ui, state);
    }
    ply.show(|_| {}).await;
}

pub fn evaluate_headless_state(state: &PlaygroundState) -> FrameEvidence {
    let mut ply = Ply::<()>::new_headless(Dimensions::new(1200.0, 800.0));
    ply.set_measure_text_function(|text, config| {
        let size = f32::from(config.font_size.max(12));
        Dimensions::new(text.chars().count() as f32 * size * 0.56, size * 1.25)
    });
    evaluate_frame(&mut ply, state)
}

pub fn simulate_interactions() -> InteractionEvidence {
    let mut state = PlaygroundState::new();
    let selected_initial = state.selected_name().to_owned();
    state.select_next();
    let selected_after_next = state.selected_name().to_owned();

    let counter_index = state.counter_index().unwrap_or(0);
    let counter_initial = render_text(&state.examples[counter_index].output);
    state.trigger_counter_increment();
    let counter_after_increment = render_text(&state.examples[counter_index].output);
    state.trigger_counter_increment();
    let counter_after_second_increment = render_text(&state.examples[counter_index].output);

    let generic_index = state
        .examples
        .iter()
        .position(|example| !matches!(example.name.as_str(), "counter" | "counter_hold"))
        .unwrap_or(0);
    state.selected = generic_index;
    let generic_initial = render_text(&state.examples[generic_index].output);
    state.trigger_selected();
    let generic_after_activation = render_text(&state.examples[generic_index].output);
    let generic_state_changed_or_noop_documented = generic_initial != generic_after_activation
        || state.examples[generic_index].scenario_steps.is_empty()
        || state.examples[generic_index]
            .scenario_steps
            .iter()
            .all(|step| step.actions.is_empty());

    let selection_changed = state.examples.len() < 2 || selected_after_next != selected_initial;
    let counter_state_changed = counter_after_increment != counter_initial;
    let counter_advanced_twice = counter_after_second_increment != counter_after_increment;

    InteractionEvidence {
        selected_initial,
        selected_after_next,
        counter_initial,
        counter_after_increment: counter_after_increment.clone(),
        counter_after_second_increment: counter_after_second_increment.clone(),
        generic_initial,
        generic_after_activation,
        selection_changed,
        counter_state_changed,
        counter_advanced_twice,
        generic_state_changed_or_noop_documented,
    }
}

fn frame_evidence_from_commands(
    state: &PlaygroundState,
    commands: &[ply_engine::render_commands::RenderCommand<()>],
) -> FrameEvidence {
    let mut rectangles = 0;
    let mut texts = 0;
    let mut borders = 0;
    let mut ids = BTreeSet::new();
    for command in commands {
        ids.insert(command.id);
        match command.config {
            RenderCommandConfig::Rectangle(_) => rectangles += 1,
            RenderCommandConfig::Text(_) => texts += 1,
            RenderCommandConfig::Border(_) => borders += 1,
            _ => {}
        }
    }
    FrameEvidence {
        selected_example: state.selected_name().to_owned(),
        render_output_text: state.selected_output_text(),
        ply_render_commands: commands.len(),
        rectangles,
        texts,
        borders,
        unique_ids: ids.len(),
        semantic_widgets: semantic_widgets(state),
    }
}

fn build_playground_examples() -> Vec<PlaygroundExample> {
    generated_manifest_examples()
        .into_iter()
        .map(|(name, scenario_steps, runtime, checked_scenario_output)| {
            let initial_step_applied = usize::from(!scenario_steps.is_empty());
            let initial_actions = scenario_steps
                .first()
                .map(|step| step.actions.clone())
                .unwrap_or_default();
            let output = runtime
                .submit_actions_and_drain(&initial_actions, 1)
                .unwrap_or_else(|_| empty_dd_output());
            PlaygroundExample {
                name: name.to_owned(),
                scenario_steps,
                runtime,
                step_index: initial_step_applied,
                next_epoch: 2,
                output,
                checked_scenario_output,
                last_auto_tick: 0.0,
            }
        })
        .collect()
}

macro_rules! generated_example {
    ($name:literal, $crate_name:ident) => {
        (
            $name,
            $crate_name::checked_scenario_steps(),
            LiveExampleRuntime::generated(
                $crate_name::GeneratedGraphSession::new(),
                $crate_name::GeneratedGraphSession::submit_actions_and_drain,
                $crate_name::GeneratedGraphSession::graph_builds,
                $crate_name::GeneratedGraphSession::drained_outputs,
            ),
            $crate_name::run_checked_scenario(),
        )
    };
}

fn generated_manifest_examples() -> Vec<(
    &'static str,
    Vec<ScenarioStep>,
    LiveExampleRuntime,
    SmokeOutput,
)> {
    vec![
        generated_example!("counter", generated_counter),
        generated_example!("counter_hold", generated_counter_hold),
        generated_example!("interval", generated_interval),
        generated_example!("interval_hold", generated_interval_hold),
        generated_example!("latest", generated_latest),
        generated_example!("when", generated_when),
        generated_example!("while", generated_while),
        generated_example!("then", generated_then),
        generated_example!("list_map_block", generated_list_map_block),
        generated_example!("list_map_external_dep", generated_list_map_external_dep),
        generated_example!("list_object_state", generated_list_object_state),
        generated_example!("list_retain_count", generated_list_retain_count),
        generated_example!("list_retain_reactive", generated_list_retain_reactive),
        generated_example!("list_retain_remove", generated_list_retain_remove),
        generated_example!("shopping_list", generated_shopping_list),
        generated_example!("todo_mvc", generated_todo_mvc),
        generated_example!("crud", generated_crud),
        generated_example!("flight_booker", generated_flight_booker),
        generated_example!("temperature_converter", generated_temperature_converter),
        generated_example!("pong", generated_pong),
        generated_example!("cells", generated_cells),
        generated_example!("todo_mvc_physical", generated_todo_mvc_physical),
    ]
}

fn trigger_example_action(example: &mut PlaygroundExample) {
    let actions = example
        .scenario_steps
        .get(
            example
                .step_index
                .min(example.scenario_steps.len().saturating_sub(1)),
        )
        .map(|step| step.actions.clone())
        .unwrap_or_default();
    let output = example
        .runtime
        .submit_actions_and_drain(&actions, example.next_epoch)
        .unwrap_or_else(|_| example.output.clone());
    example.next_epoch += 1;
    example.step_index += 1;
    example.output = output;
}

fn empty_dd_output() -> SmokeOutput {
    SmokeOutput {
        monitor: Vec::new(),
        render: Vec::new(),
        effects: Vec::new(),
        persistence: Vec::new(),
    }
}

pub fn render_text(output: &SmokeOutput) -> String {
    output
        .render
        .first()
        .map(|command| match command {
            boon_dd::RenderCommand::PatchText { text, .. } => text.clone(),
        })
        .unwrap_or_default()
}

fn build_ui(ui: &mut Ui<'_, ()>, state: &PlaygroundState) {
    ui.element()
        .width(grow!())
        .height(grow!())
        .background_color(0xEEF1F5)
        .layout(|layout| layout.direction(LeftToRight))
        .children(|ui| {
            sidebar(ui, state);
            selected_panel(ui, state);
        });
}

fn sidebar(ui: &mut Ui<'_, ()>, state: &PlaygroundState) {
    ui.element()
        .id("sidebar")
        .width(fixed!(SIDEBAR_WIDTH))
        .height(grow!())
        .background_color(0x1F2630)
        .layout(|layout| {
            layout
                .direction(TopToBottom)
                .padding((18, 18, 18, 18))
                .gap(8)
        })
        .children(|ui| {
            ui.text("BOON DD", |text| text.font_size(30).color(0xF1F5FA));
            ui.text("PLY PLAYGROUND", |text| text.font_size(16).color(0x9FB0C7));
            for index in visible_example_range(state) {
                let example = &state.examples[index];
                let selected = index == state.selected;
                ui.element()
                    .id(Id::new_index("example", index as u32))
                    .width(grow!())
                    .height(fixed!(34.0))
                    .background_color(if selected { 0x2F6FB8 } else { 0x28313D })
                    .layout(|layout| layout.padding((10, 10, 8, 8)).align(Left, CenterY))
                    .children(|ui| {
                        let label =
                            format!("{:02} {}", index + 1, short_example_name(&example.name));
                        ui.text(&label, |text| text.font_size(15).color(0xF1F5FA));
                    });
            }
            let footer = format!("{} OF {}", state.selected + 1, state.examples.len());
            ui.text(&footer, |text| text.font_size(18).color(0xDCE7F5));
        });
}

fn selected_panel(ui: &mut Ui<'_, ()>, state: &PlaygroundState) {
    ui.element()
        .id("selected-panel")
        .width(grow!())
        .height(grow!())
        .background_color(0xEEF1F5)
        .layout(|layout| {
            layout
                .direction(TopToBottom)
                .padding((34, 34, 30, 30))
                .gap(18)
        })
        .children(|ui| {
            if let Some(example) = state.examples.get(state.selected) {
                evidence_bar(ui, state, example);
                example_view(ui, example, state.selected, state.examples.len());
            }
        });
}

fn example_view(ui: &mut Ui<'_, ()>, example: &PlaygroundExample, selected: usize, total: usize) {
    workbench(ui, example, selected, total);
}

fn evidence_bar(ui: &mut Ui<'_, ()>, state: &PlaygroundState, example: &PlaygroundExample) {
    let output = render_text(&example.output);
    let label = format!(
        "Runtime output: {}  |  Actions: {}  |  Monitor records: {}  |  Interactions: {}  |  Input: {}  |  Submitted: {}",
        if output.is_empty() {
            "<empty>"
        } else {
            output.as_str()
        },
        example.step_index,
        example.output.monitor.len(),
        state.interaction_log.len(),
        if state.input_buffer.is_empty() {
            "<empty>"
        } else {
            state.input_buffer.as_str()
        },
        if state.last_submitted_text.is_empty() {
            "<empty>"
        } else {
            state.last_submitted_text.as_str()
        }
    );
    ui.element()
        .id("runtime-evidence")
        .width(grow!())
        .height(fixed!(56.0))
        .background_color(0xDDE6F1)
        .layout(|layout| layout.padding((14, 14, 10, 10)).align(Left, CenterY))
        .children(|ui| {
            ui.text(&label, |text| text.font_size(18).color(0x1F2630));
        });
}

fn workbench(ui: &mut Ui<'_, ()>, example: &PlaygroundExample, selected: usize, total: usize) {
    title(ui, &example.name);
    card(ui, "Generated DD Output", |ui| {
        let value = render_text(&example.output);
        ui.text("Render text", |text| text.font_size(18).color(0x2F6FB8));
        ui.text(if value.is_empty() { "<empty>" } else { &value }, |text| {
            text.font_size(34).color(0x343A40)
        });
        let monitor = format!("Monitor records: {}", example.output.monitor.len());
        ui.text(&monitor, |text| text.font_size(18).color(0x6B7280));
        let render_commands = format!("Render commands: {}", example.output.render.len());
        ui.text(&render_commands, |text| text.font_size(18).color(0x6B7280));
        button_with_id(ui, "example-activate", "Activate", 0x2F6FB8);
        let loaded = format!("Loaded example {} of {}", selected + 1, total);
        ui.text(&loaded, |text| text.font_size(18).color(0x6B7280));
    });
}

fn title(ui: &mut Ui<'_, ()>, title: &str) {
    ui.text(title, |text| text.font_size(42).color(0xB7474B));
}

fn card(ui: &mut Ui<'_, ()>, label: &str, children: impl FnOnce(&mut Ui<'_, ()>)) {
    ui.element()
        .width(grow!())
        .height(fit!())
        .background_color(0xFFFFFF)
        .corner_radius(8.0)
        .layout(|layout| {
            layout
                .direction(TopToBottom)
                .padding((24, 24, 22, 22))
                .gap(14)
        })
        .children(|ui| {
            ui.text(label, |text| text.font_size(20).color(0x2F6FB8));
            children(ui);
        });
}

fn button_with_id(ui: &mut Ui<'_, ()>, id: &'static str, label: &str, color: u32) {
    button_inner(ui, Some(id), label, color);
}

fn button_inner(ui: &mut Ui<'_, ()>, id: Option<&'static str>, label: &str, color: u32) {
    let element = ui
        .element()
        .width(fit!(88.0, 210.0))
        .height(fixed!(42.0))
        .background_color(color)
        .corner_radius(6.0)
        .layout(|layout| layout.padding((16, 16, 10, 10)).align(CenterX, CenterY));
    let element = if let Some(id) = id {
        element.id(id)
    } else {
        element
    };
    element.children(|ui| {
        ui.text(label, |text| text.font_size(17).color(0xFFFFFF));
    });
}

fn semantic_widgets(state: &PlaygroundState) -> Vec<String> {
    vec![
        "sidebar".to_owned(),
        "example_labels".to_owned(),
        "selected_output_panel".to_owned(),
        "generated_render_text".to_owned(),
        "generated_monitor_count".to_owned(),
        "generic_activate_control".to_owned(),
        format!("selected:{}", state.selected_name()),
    ]
}

fn short_example_name(name: &str) -> &str {
    match name {
        "list_map_external_dep" => "list_map_ext",
        "list_retain_reactive" => "list_retain_react",
        "temperature_converter" => "temperature",
        "todo_mvc_physical" => "todo_physical",
        other => other,
    }
}

fn visible_example_range(state: &PlaygroundState) -> std::ops::Range<usize> {
    let start = state
        .selected
        .saturating_sub(VISIBLE_ROWS / 2)
        .min(state.examples.len().saturating_sub(VISIBLE_ROWS));
    let end = (start + VISIBLE_ROWS).min(state.examples.len());
    start..end
}
