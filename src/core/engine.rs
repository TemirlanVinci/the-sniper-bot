// src/core/engine.rs
use crate::connectors::traits::ExecutionHandler;
use crate::strategies::traits::Strategy;
use crate::types::{Position, Side, Signal, Ticker, UiEvent};
use anyhow::Result;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fs;
use std::str::FromStr;
use tokio::sync::mpsc;
use tracing::{error, info, warn}; // Added warn

// Простое персистентное состояние
#[derive(Debug, Default, Serialize, Deserialize)]
struct EngineState {
    active_position: Option<Position>,
}

pub struct TradingEngine<S> {
    execution_handler: Box<dyn ExecutionHandler>,
    strategy: S,
    ticker_receiver: mpsc::Receiver<Ticker>,
    ui_sender: mpsc::Sender<UiEvent>,
    live_mode: bool,
    state_file: String,
}

impl<S> TradingEngine<S>
where
    S: Strategy,
{
    pub fn new(
        execution_handler: Box<dyn ExecutionHandler>,
        strategy: S,
        ticker_receiver: mpsc::Receiver<Ticker>,
        ui_sender: mpsc::Sender<UiEvent>,
        live_mode: bool,
    ) -> Self {
        Self {
            execution_handler,
            strategy,
            ticker_receiver,
            ui_sender,
            live_mode,
            state_file: "bot_state.json".to_string(),
        }
    }

    fn load_state(&mut self) {
        if let Ok(data) = fs::read_to_string(&self.state_file) {
            if let Ok(state) = serde_json::from_str::<EngineState>(&data) {
                info!("Restored state: {:?}", state);
                self.strategy.update_position(state.active_position);
            }
        }
    }

    fn save_state(&self, position: Option<Position>) {
        let state = EngineState {
            active_position: position,
        };
        if let Ok(data) = serde_json::to_string_pretty(&state) {
            let _ = fs::write(&self.state_file, data);
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        info!("Engine starting...");
        self.load_state();
        self.strategy.init().await?;

        info!("Engine loop running. Live Mode: {}", self.live_mode);

        while let Some(ticker) = self.ticker_receiver.recv().await {
            // Forward ticker to UI
            let _ = self
                .ui_sender
                .send(UiEvent::TickerUpdate(ticker.clone()))
                .await;

            // Strategy Tick
            let signal = self.strategy.on_tick(&ticker).await?;

            match signal {
                Signal::Advice(side, price) => {
                    self.handle_signal(side, price, &ticker).await?;
                }
                Signal::Hold => {}
            }
        }
        Ok(())
    }

    async fn handle_signal(
        &mut self,
        side: Side,
        current_price: Decimal,
        ticker: &Ticker,
    ) -> Result<()> {
        info!("Signal detected: {:?} @ {}", side, current_price);
        let _ = self
            .ui_sender
            .send(UiEvent::Signal(Signal::Advice(side, current_price)))
            .await;

        if !self.live_mode {
            // Simulation
            let fake_pos = match side {
                Side::Buy => Some(Position {
                    symbol: ticker.symbol.clone(),
                    quantity: Decimal::from_str("0.001").unwrap(),
                    entry_price: current_price,
                    unrealized_pnl: Decimal::ZERO,
                    highest_price: current_price,
                }),
                Side::Sell => None,
            };
            self.strategy.update_position(fake_pos.clone());
            self.save_state(fake_pos);
            return Ok(());
        }

        // --- LIVE EXECUTION LOGIC ---
        // TODO: Перенести размер лота в конфиг
        let quantity = Decimal::from_str("0.002").unwrap();

        let required_asset = match side {
            Side::Buy => "USDT",
            Side::Sell => "BTC",
        };

        // 1. Balance Check
        let balance = self
            .execution_handler
            .get_balance(required_asset)
            .await
            .unwrap_or(Decimal::ZERO);

        let required_amt = match side {
            Side::Buy => quantity * current_price / Decimal::from(5),
            Side::Sell => quantity,
        };

        if balance < required_amt {
            // info!("Warning: Local balance check low, trying anyway...");
        }

        // 2. Place Order (Limit IOC)
        let slippage = Decimal::from_str("0.001").unwrap(); // 0.1%
        let limit_price = match side {
            Side::Buy => current_price * (Decimal::ONE + slippage),
            Side::Sell => current_price * (Decimal::ONE - slippage),
        };
        let limit_price = limit_price.round_dp(2);

        // FIXED: Only update state if the order was ACTUALLY filled
        match self
            .execution_handler
            .place_order(&ticker.symbol, side, quantity, Some(limit_price))
            .await
        {
            Ok(order) => {
                info!("✅ Order Confirmed & Filled: {:?}", order);
                match side {
                    Side::Buy => {
                        let pos = Position {
                            symbol: ticker.symbol.clone(),
                            quantity,
                            entry_price: limit_price,
                            unrealized_pnl: Decimal::ZERO,
                            highest_price: limit_price,
                        };
                        self.strategy.update_position(Some(pos.clone()));
                        self.save_state(Some(pos));
                    }
                    Side::Sell => {
                        self.strategy.update_position(None);
                        self.save_state(None);
                    }
                }
            }
            Err(e) => {
                // FIXED: Log warning and DO NOT update strategy state
                warn!("⚠️ Slippage too high, entry missed or error: {}", e);
            }
        }

        Ok(())
    }
}
