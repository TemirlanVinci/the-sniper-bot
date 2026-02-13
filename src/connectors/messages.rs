// src/connectors/messages.rs
use rust_decimal::Decimal;
use serde::Deserialize;

// Для чтения потока @bookTicker (OBI - Order Book Imbalance)
#[derive(Debug, Deserialize)]
pub struct BookTickerEvent {
    #[serde(rename = "b")]
    pub best_bid_price: Decimal,
    #[serde(rename = "B")]
    pub best_bid_qty: Decimal,
    #[serde(rename = "a")]
    pub best_ask_price: Decimal,
    #[serde(rename = "A")]
    pub best_ask_qty: Decimal,
    #[serde(rename = "E")]
    pub event_time: u64,
}

// Старая структура (можно оставить для совместимости, если вдруг понадобится)
#[derive(Debug, Deserialize)]
pub struct BinanceTradeEvent {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "p")]
    pub price: Decimal,
    #[serde(rename = "q")]
    pub quantity: Decimal,
}
