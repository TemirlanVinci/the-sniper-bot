// src/tui/mod.rs
use crate::types::{Signal, UiEvent};
use anyhow::Result;
use chrono::{DateTime, Local};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
    Frame, Terminal,
};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::io;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub struct App {
    receiver: mpsc::Receiver<UiEvent>,
    symbol: String,
    // State
    price: Decimal,
    rsi: f64,
    obi: Decimal,
    // PnL in decimal percentage (e.g. 0.01 for 1%)
    pnl: Option<Decimal>,
    logs: Vec<String>,
    active_signal: String, // "BUY", "SELL", "WAITING"
    start_time: Instant,
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
            start_time: Instant::now(),
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
                            self.active_signal = format!("{:?}", side).to_uppercase();
                            // Логируем сигнал для истории
                            let msg = format!("SIGNAL: {:?} @ {}", side, price);
                            self.add_log(msg);
                        }
                        Signal::StateChanged => {} // Игнорируем внутренние изменения
                        Signal::Hold => self.active_signal = "HOLD".to_string(),
                    },
                    UiEvent::Log(l) => self.add_log(l),
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

    fn add_log(&mut self, message: String) {
        let timestamp = Local::now().format("%H:%M:%S");
        self.logs.push(format!("[{}] {}", timestamp, message));
        if self.logs.len() > 20 {
            self.logs.remove(0);
        }
    }

    fn ui(&self, f: &mut Frame) {
        // Основной Layout: Header (Top), Monitor (Middle), Logs (Bottom)
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints(
                [
                    Constraint::Length(3), // Status Bar
                    Constraint::Min(10),   // Position Monitor (Flexible)
                    Constraint::Length(8), // System Logs
                ]
                .as_ref(),
            )
            .split(f.size());

        self.render_status_bar(f, chunks[0]);
        self.render_position_monitor(f, chunks[1]);
        self.render_logs(f, chunks[2]);
    }

    fn render_status_bar(&self, f: &mut Frame, area: Rect) {
        let uptime_sec = self.start_time.elapsed().as_secs();
        let uptime = format!(
            "{:02}:{:02}:{:02}",
            uptime_sec / 3600,
            (uptime_sec % 3600) / 60,
            uptime_sec % 60
        );

        // Разбиваем хедер на 3 части
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(30),
                Constraint::Percentage(40),
                Constraint::Percentage(30),
            ])
            .split(area);

        // 1. Bot Name & Version
        let title = Paragraph::new(Span::styled(
            " THE SNIPER BOT ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        );
        f.render_widget(title, chunks[0]);

        // 2. Market Status
        let market_info = format!(" {} | ${:.2}", self.symbol, self.price);
        let center_widget = Paragraph::new(Span::raw(market_info))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" Market "),
            );
        f.render_widget(center_widget, chunks[1]);

        // 3. System Status
        let status = format!(" Uptime: {} ", uptime);
        let right_widget = Paragraph::new(Span::raw(status))
            .alignment(Alignment::Right)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" System "),
            );
        f.render_widget(right_widget, chunks[2]);
    }

    fn render_position_monitor(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double) // Двойная рамка для важности
            .title(Span::styled(
                " POSITION MONITOR ",
                Style::default().add_modifier(Modifier::BOLD),
            ));

        let inner_area = block.inner(area);
        f.render_widget(block, area);

        if let Some(pnl_pct) = self.pnl {
            // --- ACTIVE POSITION LOGIC ---

            // 1. Reverse Engineering Entry Price (Приблизительно, т.к. нет Qty в Event)
            // PnL% = (Current - Entry) / Entry  => Entry = Current / (1 + PnL%)
            // Для Short позиций логика инвертируется, но пока считаем как Long для простоты визуализации,
            // либо если PnL отрицательный на росте - это шорт.
            // *Для точности лучше добавить side в Snapshot в будущем.*

            let one = Decimal::new(1, 0);
            let entry_price = if !pnl_pct.is_zero() {
                self.price / (one + pnl_pct)
            } else {
                self.price
            };

            // 2. Estimating Logic (Hardcoded 10 USDT Order Size for visualization purpose)
            let estimated_balance = Decimal::new(10, 0);
            let qty = estimated_balance / entry_price;

            // 3. Calc Metrics
            let gross_pnl = (self.price - entry_price) * qty;

            // Fees: 0.05% Entry + 0.05% Exit (Taker)
            let fee_rate = Decimal::from_f64_retain(0.0005).unwrap_or(Decimal::ZERO);
            let entry_fee = entry_price * qty * fee_rate;
            let exit_fee = self.price * qty * fee_rate;
            let total_fees = entry_fee + exit_fee;

            let net_pnl = gross_pnl - total_fees;
            let net_pnl_pct = net_pnl / estimated_balance * Decimal::new(100, 0);

            // 4. Styling
            let pnl_color = if net_pnl >= Decimal::ZERO {
                Color::Green
            } else {
                Color::Red
            };

            // 5. Layout for Monitor
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1), // Spacer
                    Constraint::Length(1), // Side & Symbol
                    Constraint::Length(1), // Prices
                    Constraint::Length(1), // Spacer
                    Constraint::Length(1), // Gross PnL
                    Constraint::Length(1), // Fees
                    Constraint::Length(2), // Spacer / Divider
                    Constraint::Length(1), // NET PNL (BIG)
                ])
                .split(inner_area);

            // Row 1: Header
            let side_str = if gross_pnl >= Decimal::ZERO {
                "LONG (Est.)"
            } else {
                "SHORT (Est.)"
            }; // Упрощение
            f.render_widget(
                Paragraph::new(format!("{} Position: {}", side_str, self.symbol))
                    .alignment(Alignment::Center)
                    .style(Style::default().add_modifier(Modifier::BOLD)),
                chunks[1],
            );

            // Row 2: Prices
            f.render_widget(
                Paragraph::new(format!(
                    "Entry: {:.4}  ->  Current: {:.4}",
                    entry_price, self.price
                ))
                .alignment(Alignment::Center),
                chunks[2],
            );

            // Row 3: Gross
            f.render_widget(
                Paragraph::new(format!("Gross PnL: {:.4} USDT", gross_pnl))
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(if gross_pnl >= Decimal::ZERO {
                        Color::Green
                    } else {
                        Color::Red
                    })),
                chunks[4],
            );

            // Row 4: Fees
            f.render_widget(
                Paragraph::new(format!("Est. Fees: -{:.4} USDT (0.1%)", total_fees))
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::Yellow)),
                chunks[5],
            );

            // Row 5: Net PnL
            let net_text = format!(" NET PNL: {:.4} USDT ({:.2}%) ", net_pnl, net_pnl_pct);
            f.render_widget(
                Paragraph::new(net_text).alignment(Alignment::Center).style(
                    Style::default()
                        .fg(Color::Black)
                        .bg(pnl_color)
                        .add_modifier(Modifier::BOLD),
                ),
                chunks[7],
            );
        } else {
            // --- IDLE STATE ---
            let center_block = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(40),
                    Constraint::Length(3),
                    Constraint::Percentage(40),
                ])
                .split(inner_area);

            let status_text = format!("WAITING FOR SIGNAL | RSI: {:.1}", self.rsi);
            let p = Paragraph::new(status_text)
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray));

            f.render_widget(p, center_block[1]);
        }
    }

    fn render_logs(&self, f: &mut Frame, area: Rect) {
        let log_items: Vec<ListItem> = self
            .logs
            .iter()
            .rev()
            .map(|l| {
                let color = if l.contains("SIGNAL") {
                    Color::Yellow
                } else if l.contains("Error") {
                    Color::Red
                } else {
                    Color::Gray
                };
                ListItem::new(Span::styled(l.clone(), Style::default().fg(color)))
            })
            .collect();

        let logs_list =
            List::new(log_items).block(Block::default().borders(Borders::TOP).title(" Logs "));
        f.render_widget(logs_list, area);
    }
}
