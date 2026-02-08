// src/strategies/traits.rs
use crate::core::types::{Position, Signal, Ticker};
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait StreamClient: Send + Sync {
    // Pass the sender so the connector knows where to push data
    async fn subscribe_ticker(&mut self, symbol: &str, sender: mpsc::Sender<Ticker>) -> Result<()>;
}
