// src/connectors/messages.rs
use rust_decimal::Decimal;
use serde::Deserialize;

/// Событие сделки (Trade) с потока wss://stream.binance.com:9443/ws/<symbol>@trade
/// Используем короткие имена полей (rename) для маппинга JSON от Binance.
#[derive(Debug, Deserialize)]
pub struct BinanceTradeEvent {
    #[serde(rename = "e")]
    pub event_type: String, // "trade"

    #[serde(rename = "E")]
    pub event_time: u64,

    #[serde(rename = "s")]
    pub symbol: String,

    #[serde(rename = "t")]
    pub trade_id: u64,

    #[serde(rename = "p")]
    pub price: Decimal,

    #[serde(rename = "q")]
    pub quantity: Decimal,

    #[serde(rename = "b")]
    pub buyer_order_id: u64,

    #[serde(rename = "a")]
    pub seller_order_id: u64,

    #[serde(rename = "T")]
    pub trade_time: u64,

    #[serde(rename = "m")]
    pub is_buyer_maker: bool,
}
