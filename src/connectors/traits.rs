// src/connectors/traits.rs
use crate::types::{OrderResponse, Side, Ticker};
use anyhow::Result;
use async_trait::async_trait;
use rust_decimal::Decimal;
use tokio::sync::mpsc;

/// Трейт для получения рыночных данных (Market Data)
#[async_trait]
pub trait StreamClient: Send + Sync {
    async fn subscribe_ticker(&mut self, symbol: &str, sender: mpsc::Sender<Ticker>) -> Result<()>;
}

/// Трейт для исполнения ордеров (Execution)
#[async_trait]
pub trait ExecutionHandler: Send + Sync {
    async fn get_balance(&self, asset: &str) -> Result<Decimal>;

    async fn place_order(
        &self,
        symbol: &str,
        side: Side,
        amount: Decimal,
        price: Option<Decimal>, // Если Some -> LIMIT IOC, если None -> MARKET (use cautiously)
    ) -> Result<OrderResponse>;

    async fn cancel_order(&self, symbol: &str, order_id: &str) -> Result<()>;
}
