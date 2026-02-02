use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticker {
    pub symbol: String,
    pub price: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResponse {
    pub id: String,
    pub symbol: String,
    pub status: String,
}
