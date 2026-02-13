// src/core/engine.rs
use crate::connectors::traits::ExchangeClient;
use crate::strategies::traits::Strategy;
use crate::types::{Position, Side, Signal, Ticker, UiEvent};
use anyhow::Result;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use tokio::sync::mpsc;
use tracing::{error, info};

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
        info!("Engine started ({})", mode);
        let _ = self
            .ui_sender
            .send(UiEvent::Log(format!("Engine started ({})", mode)))
            .await;

        self.strategy.init().await?;

        while let Some(ticker) = self.ticker_receiver.recv().await {
            let _ = self
                .ui_sender
                .send(UiEvent::TickerUpdate(ticker.clone()))
                .await;

            let signal = self.strategy.on_tick(&ticker).await?;

            match signal {
                Signal::Advice(side, current_price) => {
                    info!("Signal received: {:?} at ${}", side, current_price);
                    let _ = self
                        .ui_sender
                        .send(UiEvent::Signal(Signal::Advice(side, current_price)))
                        .await;

                    if self.live_mode {
                        let quantity = Decimal::from_str("0.0002").unwrap();

                        // --- 1. BALANCE CHECK (Requirement #4) ---
                        // Предполагаем пару BTCUSDT. Buy -> нужен USDT, Sell -> нужен BTC.
                        // В реальном коде нужно парсить символ.
                        let required_asset = match side {
                            Side::Buy => "USDT",
                            Side::Sell => "BTC",
                        };

                        match self.exchange.get_balance(required_asset).await {
                            Ok(balance) => {
                                let required_amount = match side {
                                    Side::Buy => quantity * current_price,
                                    Side::Sell => quantity,
                                };

                                if balance < required_amount {
                                    error!(
                                        "❌ INSUFFICIENT FUNDS: Have {} {}, Need {}",
                                        balance, required_asset, required_amount
                                    );
                                    let _ = self
                                        .ui_sender
                                        .send(UiEvent::Log(format!(
                                            "❌ NO FUNDS: {}",
                                            required_asset
                                        )))
                                        .await;
                                    continue; // Skip execution
                                }
                            }
                            Err(e) => {
                                error!("Failed to check balance: {}", e);
                                continue;
                            }
                        }

                        // --- 2. SLIPPAGE PROTECTION & LIMIT PRICE (Requirement #3) ---
                        // Рассчитываем Limit цену:
                        // Buy: Текущая + 0.1%
                        // Sell: Текущая - 0.1%
                        let slip_pct = Decimal::from_str("0.001").unwrap(); // 0.1%
                        let limit_price = match side {
                            Side::Buy => current_price * (Decimal::ONE + slip_pct),
                            Side::Sell => current_price * (Decimal::ONE - slip_pct),
                        };
                        let limit_price = limit_price.round_dp(2); // Округление до 2 знаков (для BTCUSDT)

                        info!("Executing LIVE {:?} with Limit Price {}", side, limit_price);

                        match self
                            .exchange
                            .place_order(&ticker.symbol, side, quantity, Some(limit_price))
                            .await
                        {
                            Ok(order) => {
                                info!("✅ ORDER FILLED: {}", order.id);
                                let _ = self
                                    .ui_sender
                                    .send(UiEvent::Log(format!("✅ FILLED: {}", order.id)))
                                    .await;

                                // Update Strategy State
                                match side {
                                    Side::Buy => {
                                        let position = Position {
                                            symbol: ticker.symbol.clone(),
                                            quantity,
                                            entry_price: order
                                                .status
                                                .eq("FILLED")
                                                .then(|| limit_price)
                                                .unwrap_or(current_price),
                                            unrealized_pnl: Decimal::ZERO,
                                        };
                                        self.strategy.update_position(Some(position));
                                    }
                                    Side::Sell => {
                                        self.strategy.update_position(None);
                                    }
                                }
                            }
                            Err(e) => {
                                error!("❌ ORDER FAILED: {}", e);
                                let _ = self
                                    .ui_sender
                                    .send(UiEvent::Log(format!("❌ FAILED: {}", e)))
                                    .await;
                            }
                        }
                    } else {
                        // Simulation Logic
                        let quantity = Decimal::from_str("0.001").unwrap();
                        match side {
                            Side::Buy => {
                                let position = Position {
                                    symbol: ticker.symbol.clone(),
                                    quantity,
                                    entry_price: current_price,
                                    unrealized_pnl: Decimal::ZERO,
                                };
                                self.strategy.update_position(Some(position));
                            }
                            Side::Sell => {
                                self.strategy.update_position(None);
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
