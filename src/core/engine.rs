// src/core/engine.rs
use anyhow::{Context, Result};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

// Import our internal modules
use crate::connectors::traits::ExchangeClient;
use crate::core::types::{Inventory, Position, Side, Signal, Ticker};
use crate::strategies::traits::Strategy;

/// The central coordinator.
/// T: The specific Exchange implementation (e.g., BinanceClient)
/// S: The specific Strategy implementation (e.g., Scalper)
pub struct TradingEngine<T, S> {
    exchange: T,
    strategy: S,
    inventory: Inventory,
    receiver: mpsc::Receiver<Ticker>,
    symbol: String,
}

impl<T, S> TradingEngine<T, S>
where
    T: ExchangeClient,
    S: Strategy,
{
    pub fn new(exchange: T, strategy: S, receiver: mpsc::Receiver<Ticker>, symbol: String) -> Self {
        Self {
            exchange,
            strategy,
            inventory: Inventory::default(), // Should ideally load from DB on boot
            receiver,
            symbol,
        }
    }

    /// The Main Event Loop.
    /// Consumes tickers from the WebSocket channel and drives the strategy.
    pub async fn run(&mut self) -> Result<()> {
        info!("Engine started for symbol: {}", self.symbol);

        // 1. Initial State Sync
        self.sync_state()
            .await
            .context("Failed to perform initial state sync")?;

        // 2. Event Loop
        while let Some(ticker) = self.receiver.recv().await {
            // A. Retrieve Context
            let current_position = self.inventory.positions.get(&self.symbol);

            // B. Ask Strategy for Advice
            let signal = self.strategy.process(&ticker, current_position).await;

            // C. Execute Logic
            match signal {
                Signal::Advice(side, amount) => {
                    self.execute_trade(side, amount, &ticker).await?;
                }
                Signal::Hold => {
                    // Optional: Update unrealized PnL in TUI or Logs
                }
            }
        }

        Ok(())
    }

    /// Helper: Syncs local inventory with the exchange (Balance & Open Orders)
    async fn sync_state(&mut self) -> Result<()> {
        let balance = self.exchange.get_balance("USDT").await?;
        self.inventory.quote_balance = balance;
        info!("State Synced. Available USDT: {}", balance);
        Ok(())
    }

    /// Helper: execution logic with safety checks
    async fn execute_trade(&mut self, side: Side, amount: f64, ticker: &Ticker) -> Result<()> {
        // 1. Pre-Flight Checks (Risk Management)
        if side == Side::Buy {
            let cost = amount * ticker.price;
            if cost > self.inventory.quote_balance {
                warn!(
                    "Signal ignored: Insufficient funds. Required: {}, Available: {}",
                    cost, self.inventory.quote_balance
                );
                return Ok(());
            }
        } else if side == Side::Sell {
            // Check if we actually have the asset to sell
            if let Some(pos) = self.inventory.positions.get(&self.symbol) {
                if pos.quantity < amount {
                    warn!("Signal ignored: Insufficient asset quantity.");
                    return Ok(());
                }
            } else {
                warn!("Signal ignored: No position to sell.");
                return Ok(());
            }
        }

        // 2. Execute on Exchange
        info!(
            "Executing {:?} order for {} units at approx price {}",
            side, amount, ticker.price
        );
        // Note: In a real HFT, we might use Limit orders closer to the book top
        match self
            .exchange
            .place_order(&self.symbol, side, amount, None)
            .await
        {
            Ok(order_response) => {
                info!("Order placed successfully: ID {}", order_response.id);
                // 3. Update Local State (Optimistic or wait for fill confirmation via separate UserDataStream)
                // For this architecture step, we trigger a sync or update state optimistically
                self.sync_state().await?;
            }
            Err(e) => error!("Failed to execute trade: {:?}", e),
        }

        Ok(())
    }
}
