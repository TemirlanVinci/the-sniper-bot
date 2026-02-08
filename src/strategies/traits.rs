// src/strategies/traits.rs
use crate::core::types::{Position, Signal, Ticker};
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait Strategy: Send + Sync {
    /// Initialize the strategy (e.g., load historical data from Sled)
    async fn init(&mut self) -> Result<()>;

    /// The Core Logic.
    ///
    /// # Arguments
    /// * `ticker` - The latest price update.
    /// * `position` - The current open position for this symbol (if any).
    ///
    /// # Returns
    /// * `Signal` - Buy, Sell, or Hold advice.
    async fn process(&mut self, ticker: &Ticker, position: Option<&Position>) -> Signal;

    /// Optional: Identify the strategy for logging
    fn name(&self) -> &str;
}
