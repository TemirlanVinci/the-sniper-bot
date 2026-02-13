// src/tui/mod.rs
use crate::types::{Signal, UiEvent};
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
    Frame, Terminal,
};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::io;
use std::time::Duration;
use tokio::sync::mpsc;

pub struct App {
    receiver: mpsc::Receiver<UiEvent>,
    symbol: String,
    // State
    price: Decimal,
    rsi: f64,
    obi: Decimal,
    pnl: Option<Decimal>,
    logs: Vec<String>,
    active_signal: String,
}

impl App {
    pub fn new(receiver: mpsc::Receiver<UiEvent>, symbol: String) -> Self {
        Self {
            receiver,
            symbol,
            price: Decimal::ZERO,
            rsi: 50.0,
            obi: Decimal::ZERO,
            pnl: None,
            logs: vec![],
            active_signal: "WAITING".to_string(),
        }
    }

    pub async fn run(mut self) -> Result<()> {
        // Setup Terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        loop {
            // Draw
            terminal.draw(|f| self.ui(f))?;

            // Input (Non-blocking check)
            if event::poll(Duration::from_millis(10))? {
                if let Event::Key(key) = event::read()? {
                    if let KeyCode::Char('q') = key.code {
                        break;
                    }
                }
            }

            // Data updates
            while let Ok(event) = self.receiver.try_recv() {
                match event {
                    UiEvent::TickerUpdate(t) => self.price = t.price,
                    UiEvent::Signal(s) => match s {
                        Signal::Advice(side, price) => {
                            self.active_signal = format!("{:?} @ {}", side, price)
                        }
                        // <--- Игнорируем внутренние сигналы в UI, чтобы не сбивать пользователя
                        Signal::StateChanged => {}
                        Signal::Hold => self.active_signal = "HOLD".to_string(),
                    },
                    UiEvent::Log(l) => {
                        self.logs.push(l);
                        if self.logs.len() > 15 {
                            self.logs.remove(0);
                        }
                    }
                    UiEvent::Snapshot(snap) => {
                        self.rsi = snap.rsi;
                        self.obi = snap.obi;
                        self.pnl = snap.position_pnl;
                    }
                }
            }
        }

        // Cleanup
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;
        Ok(())
    }

    fn ui(&self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints(
                [
                    Constraint::Length(3), // Header
                    Constraint::Length(3), // KPI Row
                    Constraint::Length(3), // Position
                    Constraint::Min(5),    // Logs
                ]
                .as_ref(),
            )
            .split(f.size());

        // 1. Header
        let title = format!(" THE SNIPER BOT | {} | {} ", self.symbol, self.price);
        let header = Paragraph::new(title)
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(header, chunks[0]);

        // 2. Indicators
        let rsi_color = if self.rsi > 70.0 {
            Color::Red
        } else if self.rsi < 30.0 {
            Color::Green
        } else {
            Color::Gray
        };
        let obi_val = self.obi.to_f64().unwrap_or(0.0);
        let obi_color = if obi_val > 0.2 {
            Color::Green
        } else if obi_val < -0.2 {
            Color::Red
        } else {
            Color::Gray
        };

        let indicators = Paragraph::new(Line::from(vec![
            Span::raw(" RSI: "),
            Span::styled(format!("{:.2}", self.rsi), Style::default().fg(rsi_color)),
            Span::raw(" | OBI: "),
            Span::styled(format!("{:.4}", self.obi), Style::default().fg(obi_color)),
            Span::raw(" | Last Sig: "),
            Span::styled(&self.active_signal, Style::default().fg(Color::Yellow)),
        ]))
        .block(Block::default().borders(Borders::ALL).title(" Indicators "));
        f.render_widget(indicators, chunks[1]);

        // 3. Position
        let pnl_text = match self.pnl {
            Some(p) => format!(" OPEN POSITION | PnL: {:.2}%", p * Decimal::from(100)),
            None => " NO POSITION".to_string(),
        };
        let pos_color = if self.pnl.is_some() {
            Color::LightGreen
        } else {
            Color::Gray
        };

        let position_widget = Paragraph::new(pnl_text)
            .style(Style::default().fg(pos_color))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Position Status "),
            );
        f.render_widget(position_widget, chunks[2]);

        // 4. Logs
        let log_items: Vec<ListItem> = self
            .logs
            .iter()
            .rev()
            .map(|l| ListItem::new(Span::raw(l)))
            .collect();
        let logs_list = List::new(log_items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" System Logs "),
        );
        f.render_widget(logs_list, chunks[3]);
    }
}
