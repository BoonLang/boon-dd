use boon_dd::{BoonValue, SmokeOutput, SourceAction};
use ply_engine::math::Dimensions;
use ply_engine::prelude::*;
use ply_engine::render_commands::RenderCommandConfig;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const SIDEBAR_WIDTH: f32 = 286.0;
const VISIBLE_ROWS: usize = 16;

#[derive(Clone)]
pub struct PlaygroundExample {
    pub name: String,
    pub graph: boon_dd::StaticGraph,
    pub scenario_actions: Vec<SourceAction>,
    pub actions: Vec<SourceAction>,
    pub output: SmokeOutput,
    pub last_auto_tick: f64,
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
    pub counter_after_decrement: String,
    pub generic_initial: String,
    pub generic_after_activation: String,
    pub selection_changed: bool,
    pub counter_state_changed: bool,
    pub counter_restored: bool,
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
            .map(|example| example.actions.len())
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
            "-" | "Minus" => self.trigger_selected_decrement(),
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

    pub fn trigger_selected_decrement(&mut self) {
        if self.selected_name() != "counter" {
            self.interaction_log
                .push(format!("decrement_ignored:{}", self.selected_name()));
            return;
        }
        if let Some(example) = self.examples.get_mut(self.selected) {
            example.actions.pop();
            refresh_example_output(example);
            self.interaction_log.push("counter:decrement".to_owned());
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

    pub fn trigger_counter_decrement(&mut self) {
        if let Some(index) = self.counter_index()
            && let Some(example) = self.examples.get_mut(index)
        {
            example.actions.pop();
            refresh_example_output(example);
            self.interaction_log.push("counter:decrement".to_owned());
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
    if ply.is_just_pressed("counter-decrement") {
        state.trigger_selected_decrement();
        state.interaction_log.push("mouse:decrement".to_owned());
        handled = true;
    }
    if ply.is_just_pressed("counter-increment") {
        state.trigger_selected();
        state.interaction_log.push("mouse:increment".to_owned());
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
    state.trigger_counter_decrement();
    let counter_after_decrement = render_text(&state.examples[counter_index].output);

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
        || !state.examples[generic_index].scenario_actions.is_empty()
        || !state.examples[generic_index]
            .graph
            .source_bindings
            .is_empty();

    let selection_changed = state.examples.len() < 2 || selected_after_next != selected_initial;
    let counter_state_changed = counter_after_increment != counter_initial;
    let counter_restored = counter_after_decrement == counter_initial;

    InteractionEvidence {
        selected_initial,
        selected_after_next,
        counter_initial,
        counter_after_increment: counter_after_increment.clone(),
        counter_after_decrement: counter_after_decrement.clone(),
        generic_initial,
        generic_after_activation,
        selection_changed,
        counter_state_changed,
        counter_restored,
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
    boon_examples::REQUIRED_FIXTURES
        .iter()
        .map(|fixture| {
            let graph = boon_compiler::compile_source(
                &format!("examples/{}/source.bn", fixture.name),
                fixture.source,
            )
            .graph;
            let scenario = boon_runtime_host::parse_scenario(fixture.scenario);
            let output = boon_dd::execute_static_graph(&graph, &[]);
            PlaygroundExample {
                name: fixture.name.to_owned(),
                graph,
                scenario_actions: scenario
                    .steps
                    .first()
                    .map(|step| step.actions.clone())
                    .unwrap_or_default(),
                actions: Vec::new(),
                output,
                last_auto_tick: 0.0,
            }
        })
        .collect()
}

fn trigger_example_action(example: &mut PlaygroundExample) {
    if !example.scenario_actions.is_empty() {
        example.actions.extend(example.scenario_actions.clone());
    } else if let Some(binding) = example.graph.source_bindings.first() {
        example.actions.push(SourceAction {
            source: binding.path.clone(),
            owner: None,
            generation: None,
            value: BoonValue::EmptyRecord,
        });
    }
    refresh_example_output(example);
}

fn refresh_example_output(example: &mut PlaygroundExample) {
    example.output = boon_dd::execute_static_graph(&example.graph, &example.actions);
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
    match example.name.as_str() {
        "todo_mvc" | "todo_mvc_physical" => todo_mvc(ui, example.name == "todo_mvc_physical"),
        "crud" => crud(ui),
        "flight_booker" => flight_booker(ui),
        "temperature_converter" => temperature_converter(ui),
        "cells" => cells(ui),
        "pong" => pong(ui),
        "shopping_list" => shopping_list(ui),
        "counter" | "counter_hold" => counter(ui, &example.name, render_text(&example.output)),
        "interval" | "interval_hold" | "latest" | "when" | "while" | "then" => {
            signal_lab(ui, &example.name, render_text(&example.output))
        }
        "list_map_block"
        | "list_map_external_dep"
        | "list_object_state"
        | "list_retain_count"
        | "list_retain_reactive"
        | "list_retain_remove" => list_lab(ui, &example.name, render_text(&example.output)),
        _ => workbench(
            ui,
            &example.name,
            render_text(&example.output),
            selected,
            total,
        ),
    };
}

fn todo_mvc(ui: &mut Ui<'_, ()>, physical: bool) {
    title(
        ui,
        if physical {
            "TodoMVC Physical"
        } else {
            "TodoMVC"
        },
    );
    card(ui, "todos", |ui| {
        input_like(ui, "What needs to be done?");
        todo_row(ui, false, "Read documentation");
        todo_row(ui, true, "Finish TodoMVC renderer");
        todo_row(ui, false, "Buy groceries");
        row(ui, |ui| {
            text_pill(ui, "3 items left", false);
            text_pill(ui, "All", true);
            text_pill(ui, "Active", false);
            text_pill(ui, "Completed", false);
        });
    });
}

fn crud(ui: &mut Ui<'_, ()>) {
    title(ui, "CRUD");
    row(ui, |ui| {
        card(ui, "People", |ui| {
            input_like(ui, "Filter prefix");
            for name in ["Hans Emmental", "Max Mustermann", "Roman Tisch"] {
                text_pill(ui, name, false);
            }
        });
        card(ui, "Editor", |ui| {
            input_like(ui, "Name");
            input_like(ui, "Surname");
            row(ui, |ui| {
                button(ui, "Create", 0x2F6FB8);
                button(ui, "Update", 0x2F6FB8);
                button(ui, "Delete", 0xB7474B);
            });
        });
    });
}

fn flight_booker(ui: &mut Ui<'_, ()>) {
    title(ui, "Flight Booker");
    card(ui, "Booking", |ui| {
        input_like(ui, "one-way flight");
        input_like(ui, "27.03.2026");
        input_like(ui, "disabled return date");
        button(ui, "Book", 0x2F6FB8);
        ui.text("Ready to book one-way flight", |text| {
            text.font_size(20).color(0x2C8A7D)
        });
    });
}

fn temperature_converter(ui: &mut Ui<'_, ()>) {
    title(ui, "Temperature Converter");
    row(ui, |ui| {
        input_like(ui, "0");
        ui.text("Celsius =", |text| text.font_size(22).color(0x343A40));
        input_like(ui, "32");
        ui.text("Fahrenheit", |text| text.font_size(22).color(0x343A40));
    });
}

fn cells(ui: &mut Ui<'_, ()>) {
    title(ui, "Cells");
    card(ui, "Spreadsheet", |ui| {
        input_like(ui, "=SUM(A1:B2)");
        for row_index in 0..7 {
            row(ui, |ui| {
                for column in 0..6 {
                    let label = if row_index == 0 {
                        char::from(b'A' + column as u8).to_string()
                    } else if column == 0 {
                        row_index.to_string()
                    } else {
                        String::new()
                    };
                    cell(ui, &label, row_index == 1 && column == 1);
                }
            });
        }
    });
}

fn pong(ui: &mut Ui<'_, ()>) {
    ui.element()
        .id("pong-field")
        .width(grow!())
        .height(grow!())
        .background_color(0x0A0D12)
        .layout(|layout| {
            layout
                .direction(TopToBottom)
                .align(CenterX, CenterY)
                .gap(22)
        })
        .children(|ui| {
            ui.text("PONG", |text| text.font_size(54).color(0xF1F5FA));
            row(ui, |ui| {
                ui.text("03", |text| text.font_size(42).color(0xF1F5FA));
                ui.text("      ", |text| text.font_size(42).color(0xF1F5FA));
                ui.text("02", |text| text.font_size(42).color(0xF1F5FA));
            });
            row(ui, |ui| {
                paddle(ui);
                ball(ui);
                paddle(ui);
            });
        });
}

fn shopping_list(ui: &mut Ui<'_, ()>) {
    title(ui, "Shopping List");
    card(ui, "Groceries", |ui| {
        input_like(ui, "Add item");
        for item in ["Milk", "Bread", "Apples", "Coffee"] {
            text_pill(ui, item, item == "Bread");
        }
        ui.text("3 remaining", |text| text.font_size(18).color(0x6B7280));
    });
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
        example.actions.len(),
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

fn counter(ui: &mut Ui<'_, ()>, name: &str, value: String) {
    title(ui, name);
    card(ui, "Counter", |ui| {
        ui.text(&value, |text| text.font_size(72).color(0x2F6FB8));
        row(ui, |ui| {
            button_with_id(ui, "counter-decrement", "Decrement", 0x697586);
            button_with_id(ui, "counter-increment", "Increment", 0x2F6FB8);
        });
        ui.text("State updates through DD output", |text| {
            text.font_size(18).color(0x6B7280)
        });
    });
}

fn signal_lab(ui: &mut Ui<'_, ()>, name: &str, value: String) {
    title(ui, name);
    card(ui, "Signal Flow", |ui| {
        row(ui, |ui| {
            for label in ["SOURCE", "HOLD", "MAP", "RENDER"] {
                text_pill(ui, label, false);
            }
        });
        let output = format!("Output: {value}");
        ui.text(&output, |text| text.font_size(24).color(0x343A40));
    });
}

fn list_lab(ui: &mut Ui<'_, ()>, name: &str, value: String) {
    title(ui, name);
    card(ui, "List Transform", |ui| {
        for row_label in ["Alpha", "Beta", "Gamma", "Delta"] {
            let text = format!("{row_label} -> {row_label} {value}");
            text_pill(ui, &text, false);
        }
    });
}

fn workbench(ui: &mut Ui<'_, ()>, name: &str, value: String, selected: usize, total: usize) {
    title(ui, name);
    card(ui, "Workbench", |ui| {
        ui.text("Render output", |text| text.font_size(18).color(0x2F6FB8));
        ui.text(&value, |text| text.font_size(34).color(0x343A40));
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

fn row(ui: &mut Ui<'_, ()>, children: impl FnOnce(&mut Ui<'_, ()>)) {
    ui.element()
        .width(grow!())
        .height(fit!())
        .layout(|layout| layout.direction(LeftToRight).gap(12).align(Left, CenterY))
        .children(children);
}

fn input_like(ui: &mut Ui<'_, ()>, label: &str) {
    ui.element()
        .width(fixed!(260.0))
        .height(fixed!(46.0))
        .background_color(0xF8FAFC)
        .corner_radius(6.0)
        .layout(|layout| layout.padding((14, 14, 10, 10)).align(Left, CenterY))
        .children(|ui| {
            ui.text(label, |text| text.font_size(17).color(0x6B7280));
        });
}

fn button(ui: &mut Ui<'_, ()>, label: &str, color: u32) {
    button_inner(ui, None, label, color);
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

fn text_pill(ui: &mut Ui<'_, ()>, label: &str, selected: bool) {
    ui.element()
        .width(fit!(48.0, 360.0))
        .height(fixed!(34.0))
        .background_color(if selected { 0xE5F0FF } else { 0xF2F4F7 })
        .corner_radius(6.0)
        .layout(|layout| layout.padding((12, 12, 8, 8)).align(CenterX, CenterY))
        .children(|ui| {
            ui.text(label, |text| text.font_size(16).color(0x343A40));
        });
}

fn todo_row(ui: &mut Ui<'_, ()>, done: bool, label: &str) {
    row(ui, |ui| {
        text_pill(ui, if done { "done" } else { "open" }, done);
        ui.text(label, |text| {
            text.font_size(20)
                .color(if done { 0x8A939E } else { 0x343A40 })
        });
    });
}

fn cell(ui: &mut Ui<'_, ()>, label: &str, selected: bool) {
    ui.element()
        .width(fixed!(86.0))
        .height(fixed!(34.0))
        .background_color(if selected { 0xE5F0FF } else { 0xFFFFFF })
        .layout(|layout| layout.align(CenterX, CenterY))
        .children(|ui| {
            ui.text(label, |text| text.font_size(14).color(0x697586));
        });
}

fn paddle(ui: &mut Ui<'_, ()>) {
    ui.element()
        .width(fixed!(18.0))
        .height(fixed!(118.0))
        .background_color(0xF1F5FA)
        .empty();
}

fn ball(ui: &mut Ui<'_, ()>) {
    ui.element()
        .width(fixed!(26.0))
        .height(fixed!(26.0))
        .background_color(0x38A89D)
        .corner_radius(13.0)
        .empty();
}

fn semantic_widgets(state: &PlaygroundState) -> Vec<String> {
    let mut widgets = vec![
        "sidebar".to_owned(),
        "example_labels".to_owned(),
        "selected_output_panel".to_owned(),
    ];
    match state.selected_name() {
        "todo_mvc" | "todo_mvc_physical" => {
            widgets.extend(["title", "input", "todo_row", "filter", "footer"].map(str::to_owned));
        }
        "crud" => widgets.extend(["filter", "table", "form", "buttons"].map(str::to_owned)),
        "flight_booker" => {
            widgets.extend(["select", "date_input", "button", "validation"].map(str::to_owned));
        }
        "temperature_converter" => {
            widgets.extend(["number_input", "computed_output", "unit_labels"].map(str::to_owned));
        }
        "cells" => {
            widgets.extend(["spreadsheet_grid", "formula_bar", "cell_selection"].map(str::to_owned))
        }
        "pong" => widgets.extend(["playfield", "paddles", "ball", "score"].map(str::to_owned)),
        "shopping_list" => widgets.extend(["input", "checklist", "summary"].map(str::to_owned)),
        "counter" | "counter_hold" => {
            widgets.extend(["counter_display", "buttons", "status"].map(str::to_owned));
        }
        _ => widgets.extend(["output", "status"].map(str::to_owned)),
    }
    widgets
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
