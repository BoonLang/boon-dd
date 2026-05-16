use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::{CrosstermBackend, TestBackend};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::{Frame, Terminal};
use std::env;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

struct TerminalFixture {
    name: &'static str,
    source_path: &'static str,
    source: &'static str,
    scenario: &'static str,
}

macro_rules! terminal_fixture {
    ($name:literal) => {
        TerminalFixture {
            name: $name,
            source_path: concat!("examples/", $name, "/source.bn"),
            source: include_str!(concat!("../../../../examples/", $name, "/source.bn")),
            scenario: include_str!(concat!("../../../../examples/", $name, "/scenario.toml")),
        }
    };
}

const TERMINAL_FIXTURES: &[TerminalFixture] = &[
    terminal_fixture!("counter"),
    terminal_fixture!("counter_hold"),
    terminal_fixture!("interval"),
    terminal_fixture!("interval_hold"),
    terminal_fixture!("latest"),
    terminal_fixture!("when"),
    terminal_fixture!("while"),
    terminal_fixture!("then"),
    terminal_fixture!("list_map_block"),
    terminal_fixture!("list_map_external_dep"),
    terminal_fixture!("list_object_state"),
    terminal_fixture!("list_retain_count"),
    terminal_fixture!("list_retain_reactive"),
    terminal_fixture!("list_retain_remove"),
    terminal_fixture!("shopping_list"),
    terminal_fixture!("todo_mvc"),
    terminal_fixture!("crud"),
    terminal_fixture!("flight_booker"),
    terminal_fixture!("temperature_converter"),
    terminal_fixture!("pong"),
    terminal_fixture!("cells"),
    terminal_fixture!("todo_mvc_physical"),
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("--smoke") => {
            let artifact = args
                .next()
                .map(PathBuf::from)
                .ok_or("usage: terminal_playground --smoke <artifact-json>")?;
            write_smoke_artifact(artifact)?;
        }
        Some(other) => return Err(format!("unknown terminal_playground argument: {other}").into()),
        None => run_interactive()?,
    }
    Ok(())
}

fn write_smoke_artifact(artifact: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let examples = terminal_examples()?;
    let mut terminal = Terminal::new(TestBackend::new(120, 40))?;
    terminal.draw(|frame| render_playground(frame, &examples, 0))?;
    let preview = buffer_preview(terminal.backend());
    let nonblank_cells = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .filter(|cell| cell.symbol() != " ")
        .count();
    let selected_after_next = if examples.len() > 1 {
        examples[1].0.as_str()
    } else {
        examples[0].0.as_str()
    };
    let details = serde_json::json!({
        "backend": "ratatui",
        "mode": "playground",
        "interactive_controls": ["up", "down", "enter", "q"],
        "loaded_examples": examples.iter().map(|(name, _)| name).collect::<Vec<_>>(),
        "example_count": examples.len(),
        "selected_initial": examples.first().map(|(name, _)| name),
        "selected_after_simulated_down": selected_after_next,
        "ratatui_test_backend": {
            "width": 120,
            "height": 40,
            "nonblank_cells": nonblank_cells
        },
        "frame_preview": preview,
    });
    std::fs::write(artifact, serde_json::to_vec_pretty(&details)?)?;
    Ok(())
}

fn run_interactive() -> Result<(), Box<dyn std::error::Error>> {
    let examples = terminal_examples()?;
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut selected = 0_usize;
    loop {
        terminal.draw(|frame| render_playground(frame, &examples, selected))?;
        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(key) if key.code == KeyCode::Char('q') => break,
                Event::Key(key) if key.code == KeyCode::Esc => break,
                Event::Key(key) if key.code == KeyCode::Down => {
                    selected = (selected + 1).min(examples.len().saturating_sub(1));
                }
                Event::Key(key) if key.code == KeyCode::Up => {
                    selected = selected.saturating_sub(1);
                }
                Event::Key(key) if key.code == KeyCode::Enter => {}
                _ => {}
            }
        }
    }
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn terminal_examples() -> Result<Vec<(String, boon_dd::SmokeOutput)>, Box<dyn std::error::Error>> {
    TERMINAL_FIXTURES
        .iter()
        .map(|fixture| {
            let output = boon_runtime_host::run_compiled_source_scenario(
                fixture.source_path,
                fixture.source,
                fixture.scenario,
            )
            .map_err(|error| format!("failed to run compiled fixture {}: {error}", fixture.name))?;
            Ok((fixture.name.to_owned(), output))
        })
        .collect()
}

fn render_playground(
    frame: &mut Frame<'_>,
    examples: &[(String, boon_dd::SmokeOutput)],
    selected: usize,
) {
    let areas = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(32), Constraint::Min(20)])
        .split(frame.area());
    let items = examples
        .iter()
        .map(|(name, _)| ListItem::new(name.clone()))
        .collect::<Vec<_>>();
    let mut state = ListState::default().with_selected(Some(selected));
    let list = List::new(items)
        .block(Block::default().title("Examples").borders(Borders::ALL))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, areas[0], &mut state);

    let selected_output = examples
        .get(selected)
        .and_then(|(_, output)| output.render.first())
        .map(|command| match command {
            boon_dd::RenderCommand::PatchText { text, .. } => text.as_str(),
        })
        .unwrap_or("");
    let body = format!(
        "Selected: {}\n\nRender output:\n{}\n\nMonitor entries: {}",
        examples
            .get(selected)
            .map(|(name, _)| name.as_str())
            .unwrap_or(""),
        selected_output,
        examples
            .get(selected)
            .map(|(_, output)| output.monitor.len())
            .unwrap_or(0)
    );
    let paragraph =
        Paragraph::new(body).block(Block::default().title("Output").borders(Borders::ALL));
    frame.render_widget(paragraph, areas[1]);
}

fn buffer_preview(backend: &TestBackend) -> Vec<String> {
    let buffer = backend.buffer();
    let width = buffer.area().width as usize;
    buffer
        .content()
        .chunks(width)
        .take(12)
        .map(|line| {
            line.iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}
