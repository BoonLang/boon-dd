use boon_dd::{BoonValue, SmokeOutput, SourceAction};
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct TerminalExample {
    pub name: String,
    pub graph: boon_dd::StaticGraph,
    pub scenario_actions: Vec<SourceAction>,
    pub actions: Vec<SourceAction>,
    pub output: SmokeOutput,
}

#[derive(Clone)]
pub struct TerminalPlaygroundState {
    pub examples: Vec<TerminalExample>,
    pub selected: usize,
    pub input_buffer: String,
    pub last_submitted_text: String,
    pub interaction_log: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerminalScreenCapture {
    pub selected_example: String,
    pub render_output_text: String,
    pub selected_action_count: usize,
    pub selected_monitor_count: usize,
    pub input_buffer: String,
    pub last_submitted_text: String,
    pub nonblank_cells: usize,
    pub width: u16,
    pub height: u16,
    pub lines: Vec<String>,
    pub interaction_log: Vec<String>,
}

impl TerminalPlaygroundState {
    pub fn new() -> Self {
        Self {
            examples: boon_examples::REQUIRED_FIXTURES
                .iter()
                .map(|fixture| {
                    let graph = boon_compiler::compile_source(
                        &format!("examples/{}/source.bn", fixture.name),
                        fixture.source,
                    )
                    .graph;
                    let scenario = boon_runtime_host::parse_scenario(fixture.scenario);
                    let output = boon_dd::execute_static_graph(&graph, &[]);
                    TerminalExample {
                        name: fixture.name.to_owned(),
                        graph,
                        scenario_actions: scenario
                            .steps
                            .first()
                            .map(|step| step.actions.clone())
                            .unwrap_or_default(),
                        actions: Vec::new(),
                        output,
                    }
                })
                .collect(),
            selected: 0,
            input_buffer: String::new(),
            last_submitted_text: String::new(),
            interaction_log: Vec::new(),
        }
    }

    pub fn loaded_examples(&self) -> Vec<String> {
        self.examples
            .iter()
            .map(|example| example.name.clone())
            .collect()
    }

    pub fn select_by_name(&mut self, name: &str) {
        if let Some(index) = self
            .examples
            .iter()
            .position(|example| example.name == name)
        {
            self.selected = index;
            self.interaction_log.push(format!("selected:{name}"));
        }
    }

    pub fn select_next(&mut self) {
        let next = (self.selected + 1).min(self.examples.len().saturating_sub(1));
        if next != self.selected {
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

    pub fn select_first(&mut self) {
        if self.selected != 0 {
            self.selected = 0;
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
            "ArrowDown" | "Down" => self.select_next(),
            "ArrowUp" | "Up" => self.select_prev(),
            "Enter" | "Space" | " " => self.submit_selected(key),
            "-" | "Minus" => self.trigger_counter_decrement(),
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

    pub fn trigger_selected(&mut self) {
        if let Some(example) = self.examples.get_mut(self.selected) {
            trigger_example_action(example);
            self.interaction_log
                .push(format!("activated:{}", example.name));
        }
    }

    pub fn trigger_counter_decrement(&mut self) {
        if self.selected_name() != "counter" {
            return;
        }
        if let Some(example) = self.examples.get_mut(self.selected) {
            example.actions.pop();
            refresh_example_output(example);
            self.interaction_log.push("counter:decrement".to_owned());
        }
    }
}

pub fn capture_for_example(example: &str, after_action: bool) -> TerminalScreenCapture {
    let mut state = TerminalPlaygroundState::new();
    state.select_by_name(example);
    if after_action {
        state.trigger_selected();
    }
    capture_state(&state)
}

pub fn capture_state(state: &TerminalPlaygroundState) -> TerminalScreenCapture {
    let width = 120;
    let height = 40;
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| render_playground(frame, state))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let lines = buffer
        .content()
        .chunks(width as usize)
        .map(|line| {
            line.iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>();
    let nonblank_cells = buffer
        .content()
        .iter()
        .filter(|cell| cell.symbol() != " ")
        .count();
    TerminalScreenCapture {
        selected_example: state.selected_name().to_owned(),
        render_output_text: state.selected_output_text(),
        selected_action_count: state.selected_action_count(),
        selected_monitor_count: state.selected_monitor_count(),
        input_buffer: state.input_buffer.clone(),
        last_submitted_text: state.last_submitted_text.clone(),
        nonblank_cells,
        width,
        height,
        lines,
        interaction_log: state.interaction_log.clone(),
    }
}

pub fn render_playground(frame: &mut Frame<'_>, state: &TerminalPlaygroundState) {
    let areas = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(32), Constraint::Min(20)])
        .split(frame.area());
    let items = state
        .examples
        .iter()
        .map(|example| ListItem::new(example.name.clone()))
        .collect::<Vec<_>>();
    let mut list_state = ListState::default().with_selected(Some(state.selected));
    let list = List::new(items)
        .block(Block::default().title("Examples").borders(Borders::ALL))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, areas[0], &mut list_state);

    let body = format!(
        "Selected: {}\n\nRender output:\n{}\n\nActions: {}\nMonitor entries: {}\nInteractions: {}\nInput: {}\nSubmitted: {}\n\nControls: Up/Down select, Enter/Space activate, q quit",
        state.selected_name(),
        state.selected_output_text(),
        state.selected_action_count(),
        state.selected_monitor_count(),
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
        },
    );
    let paragraph =
        Paragraph::new(body).block(Block::default().title("Output").borders(Borders::ALL));
    frame.render_widget(paragraph, areas[1]);
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

fn trigger_example_action(example: &mut TerminalExample) {
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

fn refresh_example_output(example: &mut TerminalExample) {
    example.output = boon_dd::execute_static_graph(&example.graph, &example.actions);
}
