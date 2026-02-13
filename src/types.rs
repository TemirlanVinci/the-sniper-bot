// src/types.rs
use rust_decimal::Decimal;
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
    pub price: Decimal, // f64 -> Decimal
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Signal {
    Advice(Side, Decimal), // f64 -> Decimal
    Hold,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub symbol: String,
    pub quantity: Decimal,       // f64 -> Decimal
    pub entry_price: Decimal,    // f64 -> Decimal
    pub unrealized_pnl: Decimal, // f64 -> Decimal
}

#[derive(Debug, Default, Clone)]
pub struct Inventory {
    pub quote_balance: Decimal, // f64 -> Decimal
    pub positions: HashMap<String, Position>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResponse {
    pub id: String,
    pub symbol: String,
    pub status: String,
}

#[derive(Debug, Clone)]
pub enum UiEvent {
    TickerUpdate(Ticker),
    Signal(Signal),
    Log(String),
}
