// src/strategies/traits.rs
use crate::types::{Position, Signal, Ticker};
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait Strategy: Send + Sync {
    fn name(&self) -> String;

    // Initialize strategy (e.g. load history)
    async fn init(&mut self) -> Result<()>;

    // Process new tick
    async fn on_tick(&mut self, ticker: &Ticker) -> Result<Signal>;

    // Update position state (Some = open, None = closed)
    fn update_position(&mut self, position: Option<Position>);
}
