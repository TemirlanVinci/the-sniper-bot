use crate::types::{OrderResponse, Side, Ticker};
use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

#[async_trait]
pub trait ExchangeClient {
    async fn connect(&mut self) -> Result<()>;
    async fn fetch_price(&self, symbol: &str) -> Result<Ticker>;
    async fn place_order(
        &self,
        pair: &str,
        side: Side,
        amount: f64,
        price: Option<f64>,
    ) -> Result<OrderResponse>;
    async fn get_balance(&self, asset: &str) -> Result<f64>;
    async fn get_open_orders(&self, pair: &str) -> Result<Vec<OrderResponse>>;
}

#[async_trait]
pub trait StreamClient {
    // Исправили: теперь трейт знает, что нужно принимать sender
    async fn subscribe_ticker(&mut self, symbol: &str, sender: mpsc::Sender<Ticker>) -> Result<()>;
}
