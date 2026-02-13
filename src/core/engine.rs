use crate::config::AppConfig;
use crate::connectors::traits::ExecutionHandler;
use crate::strategies::traits::Strategy;
use crate::types::{Position, Side, Signal, Ticker, UiEvent};
use anyhow::Result;
use rust_decimal::prelude::FromPrimitive; // <--- ADDED THIS
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fs;
use std::str::FromStr;
use tokio::sync::mpsc;
use tracing::{info, warn};

#[derive(Debug, Default, Serialize, Deserialize)]
struct EngineState {
    active_position: Option<Position>,
}

pub struct TradingEngine<S> {
    config: AppConfig,
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
        config: AppConfig,
        execution_handler: Box<dyn ExecutionHandler>,
        strategy: S,
        ticker_receiver: mpsc::Receiver<Ticker>,
        ui_sender: mpsc::Sender<UiEvent>,
        live_mode: bool,
    ) -> Self {
        Self {
            config,
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
            let _ = self
                .ui_sender
                .send(UiEvent::TickerUpdate(ticker.clone()))
                .await;

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
            // Simulation logic
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

        // 1. Dynamic Quantity Calculation
        let order_usdt =
            Decimal::from_f64(self.config.order_size_usdt).unwrap_or(Decimal::from(10));
        let raw_qty = order_usdt / current_price;
        let quantity = self.execution_handler.normalize_quantity(raw_qty);

        info!(
            "Calculated Quantity: {} (Raw: {}) based on {} USDT",
            quantity, raw_qty, order_usdt
        );

        if quantity.is_zero() {
            warn!("⚠️ Quantity is zero (Order size too small for price). Aborting.");
            return Ok(());
        }

        // 2. Balance Check
        let required_asset = match side {
            Side::Buy => "USDT",
            Side::Sell => "BTC", // TODO: Should be base asset from config (parse from symbol)
        };

        let balance = self
            .execution_handler
            .get_balance(required_asset)
            .await
            .unwrap_or(Decimal::ZERO);

        let required_amt = match side {
            Side::Buy => quantity * current_price,
            Side::Sell => quantity,
        };

        // Optional: Add balance check logic here if strictly required
        // if balance < required_amt { ... }

        // 3. Place Order (Limit IOC with Dynamic Precision)
        let slippage = Decimal::from_str("0.001").unwrap(); // 0.1%
        let target_price = match side {
            Side::Buy => current_price * (Decimal::ONE + slippage),
            Side::Sell => current_price * (Decimal::ONE - slippage),
        };

        let final_price = self.execution_handler.normalize_price(target_price);

        match self
            .execution_handler
            .place_order(&ticker.symbol, side, quantity, Some(final_price))
            .await
        {
            Ok(order) => {
                info!("✅ Order Confirmed & Filled: {:?}", order);
                match side {
                    Side::Buy => {
                        let pos = Position {
                            symbol: ticker.symbol.clone(),
                            quantity,
                            entry_price: final_price,
                            unrealized_pnl: Decimal::ZERO,
                            highest_price: final_price,
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
                warn!("⚠️ Execution Error (Slippage/IOC/Balance): {}", e);
            }
        }

        Ok(())
    }
}
