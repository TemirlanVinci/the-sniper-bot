// src/tui/mod.rs
use crate::types::{Signal, UiEvent}; // Ticker убран, так как не используется
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
    Terminal, // Добавлен Frame
};
use rust_decimal::Decimal;
use std::io;
use std::time::Duration;
use tokio::sync::mpsc;

pub struct App {
    current_price: Option<Decimal>,
    logs: Vec<String>,
    signals: Vec<String>,
    receiver: mpsc::Receiver<UiEvent>,
    symbol: String,
}

impl App {
    pub fn new(receiver: mpsc::Receiver<UiEvent>, symbol: String) -> Self {
        Self {
            current_price: None,
            logs: vec![],
            signals: vec![],
            receiver,
            symbol,
        }
    }

    pub async fn run_loop(mut self) -> Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let tick_rate = Duration::from_millis(100);

        loop {
            // Теперь draw передает Frame без дженерика B
            terminal.draw(|f| self.ui(f))?;

            // Non-blocking event check
            if event::poll(Duration::from_millis(0))? {
                if let Event::Key(key) = event::read()? {
                    if let KeyCode::Char('q') = key.code {
                        break;
                    }
                }
            }

            // Process channel events
            while let Ok(event) = self.receiver.try_recv() {
                match event {
                    UiEvent::TickerUpdate(t) => {
                        self.current_price = Some(t.price);
                    }
                    UiEvent::Signal(s) => {
                        let msg = match s {
                            Signal::Advice(side, price) => {
                                format!("SIGNAL: {:?} @ {}", side, price)
                            }
                            Signal::Hold => "SIGNAL: HOLD".to_string(),
                        };
                        self.signals.push(msg);
                        if self.signals.len() > 20 {
                            self.signals.remove(0);
                        }
                    }
                    UiEvent::Log(l) => {
                        self.logs.push(l);
                        if self.logs.len() > 20 {
                            self.logs.remove(0);
                        }
                    }
                }
            }

            tokio::time::sleep(tick_rate).await;
        }

        // Restore terminal
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        Ok(())
    }

    // ИСПРАВЛЕНИЕ ЗДЕСЬ: Удален <B: Backend> и Frame<B> заменен на Frame
    fn ui(&self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints(
                [
                    Constraint::Length(3),      // Header
                    Constraint::Percentage(50), // Signals
                    Constraint::Percentage(50), // Logs
                ]
                .as_ref(),
            )
            .split(f.size());

        // 1. Header with Price
        let price_text = match self.current_price {
            Some(p) => format!("{:.2}", p),
            None => "Waiting...".to_string(),
        };

        let title = Paragraph::new(Line::from(vec![
            Span::styled("Sniper Bot ", Style::default().fg(Color::Green)),
            Span::styled(&self.symbol, Style::default().fg(Color::Cyan)),
            Span::raw(" | Price: "),
            Span::styled(
                price_text,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]))
        .block(Block::default().borders(Borders::ALL).title("Status"));

        f.render_widget(title, chunks[0]);

        // 2. Signals
        let signals: Vec<ListItem> = self
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

        // 3. Logs
        let logs: Vec<ListItem> = self
            .logs
            .iter()
            .rev()
            .map(|l| ListItem::new(Line::from(Span::raw(l))))
            .collect();

        let logs_list =
            List::new(logs).block(Block::default().borders(Borders::ALL).title("System Logs"));
        f.render_widget(logs_list, chunks[2]);
    }
}

pub async fn run(receiver: mpsc::Receiver<UiEvent>, symbol: String) -> Result<()> {
    let app = App::new(receiver, symbol);
    app.run_loop().await
}
