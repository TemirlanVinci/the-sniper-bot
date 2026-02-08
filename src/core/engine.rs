// src/core/engine.rs
use crate::connectors::traits::ExchangeClient;
use crate::strategies::traits::Strategy;
use crate::types::{Position, Side, Signal, Ticker};
use anyhow::Result;
use tokio::sync::mpsc;

pub struct TradingEngine<E, S> {
    exchange: E,
    strategy: S,
    ticker_receiver: mpsc::Receiver<Ticker>,
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
        live_mode: bool,
    ) -> Self {
        Self {
            exchange,
            strategy,
            ticker_receiver,
            live_mode,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        println!(
            "Starting Trading Engine (Mode: {})",
            if self.live_mode {
                "🚨 LIVE"
            } else {
                "📝 SIMULATION"
            }
        );

        self.strategy.init().await?;

        while let Some(ticker) = self.ticker_receiver.recv().await {
            let signal = self.strategy.on_tick(&ticker).await?;

            match signal {
                Signal::Advice(side, price) => {
                    println!("Signal Detected: {:?} at ${:.2}", side, price);

                    if self.live_mode {
                        // --- LIVE TRADING EXECUTION ---
                        let quantity = 0.0002; // Hardcoded safety amount (~$15-20)
                        let order_price = None; // Market Order

                        println!(
                            "🚀 EXECUTING LIVE {:?} ORDER: {} BTC on {}",
                            side, quantity, ticker.symbol
                        );

                        match self
                            .exchange
                            .place_order(&ticker.symbol, side, quantity, order_price)
                            .await
                        {
                            Ok(order) => {
                                println!("✅ LIVE ORDER SUCCESS: ID: {}", order.id);

                                // Only update strategy state if the order actually went through
                                match side {
                                    Side::Buy => {
                                        let position = Position {
                                            symbol: ticker.symbol.clone(),
                                            quantity,
                                            entry_price: ticker.price, // Use current ticker price as approx entry
                                            unrealized_pnl: 0.0,
                                        };
                                        self.strategy.update_position(Some(position));
                                        println!("Strategy State: Position OPENED");
                                    }
                                    Side::Sell => {
                                        self.strategy.update_position(None);
                                        println!("Strategy State: Position CLOSED");
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("❌ LIVE ORDER FAILED: {}", e);
                                // DO NOT update strategy state. Engine will retry on next tick if signal persists.
                            }
                        }
                    } else {
                        // --- SIMULATION LOGIC ---
                        match side {
                            Side::Buy => {
                                println!(
                                    "🔥 [SIMULATION] Executing BUY Order at ${:.2} for {}",
                                    price, ticker.symbol
                                );

                                let position = Position {
                                    symbol: ticker.symbol.clone(),
                                    quantity: 0.001,
                                    entry_price: price,
                                    unrealized_pnl: 0.0,
                                };

                                self.strategy.update_position(Some(position));
                                println!("✅ Position Opened. Strategy Updated.");
                            }
                            Side::Sell => {
                                println!(
                                    "🔥 [SIMULATION] Executing SELL Order at ${:.2} for {}",
                                    price, ticker.symbol
                                );

                                self.strategy.update_position(None);
                                println!("✅ Position Closed. Strategy Updated.");
                            }
                        }
                    }
                }
                Signal::Hold => {
                    // No action
                }
            }
        }

        Ok(())
    }
}
