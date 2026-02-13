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
    pub price: Decimal,     // Mid-Price для свечей
    pub bid_price: Decimal, // Для точного входа
    pub ask_price: Decimal,
    pub bid_qty: Decimal, // Для OBI
    pub ask_qty: Decimal, // Для OBI
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Signal {
    Advice(Side, Decimal),
    Hold,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub symbol: String,
    pub quantity: Decimal,
    pub entry_price: Decimal,
    pub unrealized_pnl: Decimal,

    // Новое поле для Trailing Stop
    // Хранит максимальную цену с момента открытия (для Long)
    pub highest_price: Decimal,
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

#[derive(Debug, Clone)]
pub enum UiEvent {
    TickerUpdate(Ticker),
    Signal(Signal),
    Log(String),
}
