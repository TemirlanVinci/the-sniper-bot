// src/connectors/traits.rs
use crate::types::{OrderResponse, Side, Ticker};
use anyhow::Result;
use async_trait::async_trait;
use rust_decimal::Decimal;
use tokio::sync::mpsc;

#[async_trait]
pub trait ExchangeClient {
    async fn connect(&mut self) -> Result<()>;
    async fn fetch_price(&self, symbol: &str) -> Result<Ticker>;

    // amount и price теперь Decimal. price обязателен для LIMIT IOC.
    async fn place_order(
        &self,
        pair: &str,
        side: Side,
        amount: Decimal,
        price: Option<Decimal>,
    ) -> Result<OrderResponse>;

    async fn get_balance(&self, asset: &str) -> Result<Decimal>;
    async fn get_open_orders(&self, pair: &str) -> Result<Vec<OrderResponse>>;
}

#[async_trait]
pub trait StreamClient {
    async fn subscribe_ticker(&mut self, symbol: &str, sender: mpsc::Sender<Ticker>) -> Result<()>;
}
