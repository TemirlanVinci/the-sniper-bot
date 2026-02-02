use crate::types::{OrderResponse, Side, Ticker};
use anyhow::Result;
use async_trait::async_trait;

// We use anyhow::Result to allow concrete implementations
// (Binance, Bybit) to propagate their specific errors up to the Engine.

#[async_trait]
pub trait ExchangeClient: Send + Sync {
    /// Initializes the connection (e.g., authenticate, ping)
    async fn connect(&mut self) -> Result<()>;

    /// Fetches the current market price for a symbol (e.g., "BTCUSDT")
    async fn fetch_price(&self, symbol: &str) -> Result<Ticker>;

    /// Places a Limit or Market order.
    /// We use a generic 'Side' enum (Buy/Sell) to avoid stringly-typed errors.
    async fn place_order(
        &self,
        pair: &str,
        side: Side,
        amount: f64,
        price: Option<f64>, // Option implies Market order if None
    ) -> Result<OrderResponse>;

    /// returns the available balance for a specific asset (e.g., "USDT")
    async fn get_balance(&self, asset: &str) -> Result<f64>;

    /// Returns the open orders for tracking
    async fn get_open_orders(&self, pair: &str) -> Result<Vec<OrderResponse>>;
}

#[async_trait]
pub trait StreamClient: Send + Sync {
    /// Subscribe to a websocket stream for real-time updates
    async fn subscribe_ticker(&mut self, symbol: &str) -> Result<()>;
}
