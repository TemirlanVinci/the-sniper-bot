// src/core/engine.rs
use crate::connectors::traits::ExchangeClient;
use crate::strategies::traits::Strategy;
use crate::types::{Position, Side, Signal, Ticker, UiEvent};
use anyhow::Result;
use tokio::sync::mpsc;

pub struct TradingEngine<E, S> {
    exchange: E,
    strategy: S,
    ticker_receiver: mpsc::Receiver<Ticker>,
    ui_sender: mpsc::Sender<UiEvent>,
    live_mode: bool,
}

impl<E, S> TradingEngine<E, S>
where
    E: ExchangeClient + Send,
    S: Strategy,
{
    pub fn new(
        exchange: E,
        strategy: S,
        ticker_receiver: mpsc::Receiver<Ticker>,
        ui_sender: mpsc::Sender<UiEvent>,
        live_mode: bool,
    ) -> Self {
        Self {
            exchange,
            strategy,
            ticker_receiver,
            ui_sender,
            live_mode,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        let mode = if self.live_mode {
            "🚨 LIVE"
        } else {
            "📝 SIMULATION"
        };
        let _ = self
            .ui_sender
            .send(UiEvent::Log(format!("Engine started ({})", mode)))
            .await;

        self.strategy.init().await?;

        while let Some(ticker) = self.ticker_receiver.recv().await {
            // 1. Update UI with latest Price
            let _ = self
                .ui_sender
                .send(UiEvent::TickerUpdate(ticker.clone()))
                .await;

            // 2. Process Strategy Logic
            let signal = self.strategy.on_tick(&ticker).await?;

            match signal {
                Signal::Advice(side, price) => {
                    let log_msg = format!("Signal: {:?} at ${:.2}", side, price);
                    let _ = self.ui_sender.send(UiEvent::Log(log_msg)).await;
                    let _ = self
                        .ui_sender
                        .send(UiEvent::Signal(Signal::Advice(side, price)))
                        .await;

                    if self.live_mode {
                        // --- LIVE TRADING ---
                        let quantity = 0.0002; // Safety limit
                        let _ = self
                            .ui_sender
                            .send(UiEvent::Log(format!(
                                "LIVE EXECUTION: {:?} {} BTC",
                                side, quantity
                            )))
                            .await;

                        // Execute Order
                        match self
                            .exchange
                            .place_order(&ticker.symbol, side, quantity, None)
                            .await
                        {
                            Ok(order) => {
                                let _ = self
                                    .ui_sender
                                    .send(UiEvent::Log(format!("✅ ORDER FILLED: {}", order.id)))
                                    .await;

                                // Update Strategy State
                                match side {
                                    Side::Buy => {
                                        let position = Position {
                                            symbol: ticker.symbol.clone(),
                                            quantity,
                                            entry_price: price, // Or use order price if available
                                            unrealized_pnl: 0.0,
                                        };
                                        // Pass ownership via Some()
                                        self.strategy.update_position(Some(position));
                                    }
                                    Side::Sell => {
                                        // Pass None to clear position
                                        self.strategy.update_position(None);
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = self
                                    .ui_sender
                                    .send(UiEvent::Log(format!("❌ ORDER FAILED: {}", e)))
                                    .await;
                            }
                        }
                    } else {
                        // --- SIMULATION ---
                        match side {
                            Side::Buy => {
                                let _ = self
                                    .ui_sender
                                    .send(UiEvent::Log("Simulating BUY...".into()))
                                    .await;
                                let position = Position {
                                    symbol: ticker.symbol.clone(),
                                    quantity: 0.001,
                                    entry_price: price,
                                    unrealized_pnl: 0.0,
                                };
                                // FIX: Wrap in Some() to match Option<Position>
                                self.strategy.update_position(Some(position));
                                let _ = self
                                    .ui_sender
                                    .send(UiEvent::Log("Position OPENED".into()))
                                    .await;
                            }
                            Side::Sell => {
                                let _ = self
                                    .ui_sender
                                    .send(UiEvent::Log("Simulating SELL...".into()))
                                    .await;
                                // FIX: Pass None to clear position
                                self.strategy.update_position(None);
                                let _ = self
                                    .ui_sender
                                    .send(UiEvent::Log("Position CLOSED".into()))
                                    .await;
                            }
                        }
                    }
                }
                Signal::Hold => {}
            }
        }
        Ok(())
    }
}
