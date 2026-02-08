// src/types.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticker {
    pub symbol: String,
    pub price: f64,
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Signal {
    Advice(Side, f64),
    Hold,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub symbol: String,
    pub quantity: f64,
    pub entry_price: f64,
    pub unrealized_pnl: f64,
}

#[derive(Debug, Default, Clone)]
pub struct Inventory {
    pub quote_balance: f64,
    pub positions: HashMap<String, Position>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResponse {
    pub id: String,
    pub symbol: String,
    pub status: String,
}

// --- Added for TUI ---
#[derive(Debug, Clone)]
pub enum UiEvent {
    TickerUpdate(Ticker),
    Signal(Signal),
    Log(String),
}
