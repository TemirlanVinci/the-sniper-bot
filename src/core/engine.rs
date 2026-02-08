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
}

impl<E, S> TradingEngine<E, S>
where
    E: ExchangeClient + Send,
    S: Strategy,
{
    pub fn new(exchange: E, strategy: S, ticker_receiver: mpsc::Receiver<Ticker>) -> Self {
        Self {
            exchange,
            strategy,
            ticker_receiver,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        println!("Starting Trading Engine...");
        self.strategy.init().await?;

        while let Some(ticker) = self.ticker_receiver.recv().await {
            let signal = self.strategy.on_tick(&ticker).await?;

            match signal {
                Signal::Advice(side, price) => {
                    match side {
                        Side::Buy => {
                            println!(
                                "🔥 [SIMULATION] Executing BUY Order at ${:.2} for {}",
                                price, ticker.symbol
                            );

                            // Simulate Order Execution & Position Creation
                            let position = Position {
                                symbol: ticker.symbol.clone(),
                                quantity: 0.001, // Simulated fixed quantity
                                entry_price: price,
                                unrealized_pnl: 0.0,
                            };

                            // CRITICAL: Update Strategy State
                            self.strategy.update_position(Some(position));
                            println!("✅ Position Opened. Strategy Updated.");
                        }
                        Side::Sell => {
                            println!(
                                "🔥 [SIMULATION] Executing SELL Order at ${:.2} for {}",
                                price, ticker.symbol
                            );

                            // Simulate Order Execution & Position Closing
                            // CRITICAL: Clear Strategy State
                            self.strategy.update_position(None);
                            println!("✅ Position Closed. Strategy Updated.");
                        }
                    }
                }
                Signal::Hold => {
                    // Logic for Hold (optional logging)
                }
            }
        }

        Ok(())
    }
}
