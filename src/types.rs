// src/core/types.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Re-export specific types if they were defined in src/types.rs previously
// Assuming basic Side/Ticker exists, we extend them here.

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

/// The output of a Strategy processing a market tick.
/// It is pure advice; the Engine decides whether to execute it.
#[derive(Debug, Clone, PartialEq)]
pub enum Signal {
    /// "I see an opportunity. Try to Buy/Sell this amount."
    Advice(Side, f64),
    /// "Do nothing."
    Hold,
}

/// Represents an currently open trade/holding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub symbol: String,
    pub quantity: f64,
    pub entry_price: f64,
    pub unrealized_pnl: f64,
}

/// The Engine's internal state tracking.
/// This prevents us from spamming the Exchange API for balance checks every millisecond.
#[derive(Debug, Default, Clone)]
pub struct Inventory {
    pub quote_balance: f64,                   // e.g., USDT available
    pub positions: HashMap<String, Position>, // Currently held assets
}
