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
    pub price: Decimal, // Mid-Price
    pub bid_price: Decimal,
    pub ask_price: Decimal,
    pub bid_qty: Decimal,
    pub ask_qty: Decimal,
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Signal {
    Advice(Side, Decimal),
    StateChanged, // <--- НОВОЕ: Сигнал изменения внутреннего состояния
    Hold,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub symbol: String,
    pub quantity: Decimal,
    pub entry_price: Decimal,
    pub unrealized_pnl: Decimal,
    pub highest_price: Decimal, // Для Trailing Stop
}

#[derive(Debug, Default, Clone)]
pub struct Inventory {
    pub quote_balance: Decimal,
    pub positions: HashMap<String, Position>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResponse {
    pub id: String,
    pub symbol: String,
    pub status: String,
}

// --- Новые структуры для TUI ---

#[derive(Debug, Clone, Default)]
pub struct StrategySnapshot {
    pub rsi: f64,
    pub obi: Decimal,
    pub position_pnl: Option<Decimal>,
}

#[derive(Debug, Clone)]
pub enum UiEvent {
    TickerUpdate(Ticker),
    Signal(Signal),
    Snapshot(StrategySnapshot),
    Log(String),
}
