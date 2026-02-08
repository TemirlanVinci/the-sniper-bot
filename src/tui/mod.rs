// src/tui/mod.rs
use crate::types::{Signal, UiEvent}; // Removed Ticker
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend, // Removed Backend
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Terminal,
};
use std::{io, time::Duration};
use tokio::sync::mpsc;

pub struct App {
    pub symbol: String,
    pub current_price: Option<f64>,
    pub signals: Vec<String>,
    pub logs: Vec<String>,
}

impl App {
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            current_price: None,
            signals: Vec::new(),
            logs: Vec::new(),
        }
    }

    pub fn on_event(&mut self, event: UiEvent) {
        match event {
            UiEvent::TickerUpdate(t) => {
                self.current_price = Some(t.price);
            }
            UiEvent::Signal(s) => match s {
                Signal::Advice(side, price) => {
                    let msg = format!("SIGNAL: {:?} at ${:.2}", side, price);
                    self.signals.push(msg);
                }
                Signal::Hold => {}
            },
            UiEvent::Log(msg) => {
                self.logs.push(msg);
                if self.logs.len() > 20 {
                    self.logs.remove(0);
                }
            }
        }
    }
}

pub async fn run(mut rx: mpsc::Receiver<UiEvent>, symbol: String) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(symbol);

    loop {
        terminal.draw(|f| ui(f, &app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if let KeyCode::Char('q') = key.code {
                    break;
                }
            }
        }

        while let Ok(event) = rx.try_recv() {
            app.on_event(event);
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

fn ui(f: &mut ratatui::Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(10),
            ]
            .as_ref(),
        )
        .split(f.size());

    let price_text = match app.current_price {
        Some(p) => format!("${:.2}", p),
        None => "Waiting for data...".to_string(),
    };

    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            format!("Sniper Bot [{}]", app.symbol),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | Price: "),
        Span::styled(
            price_text,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Status"));
    f.render_widget(header, chunks[0]);

    let signals: Vec<ListItem> = app
        .signals
        .iter()
        .rev()
        .map(|s| {
            ListItem::new(Line::from(Span::styled(
                s,
                Style::default().fg(Color::Green),
            )))
        })
        .collect();

    let signals_list = List::new(signals).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Signals History"),
    );
    f.render_widget(signals_list, chunks[1]);

    let logs: Vec<ListItem> = app
        .logs
        .iter()
        .rev()
        .map(|s| ListItem::new(Line::from(Span::raw(s))))
        .collect();

    let logs_list =
        List::new(logs).block(Block::default().borders(Borders::ALL).title("System Logs"));
    f.render_widget(logs_list, chunks[2]);
}
