// src/main.rs
use crate::connectors::binance::BinanceClient;
use crate::connectors::traits::{ExchangeClient, StreamClient};
use crate::core::engine::TradingEngine;
use crate::strategies::scalper::SimpleScalper;
use dotenvy::dotenv;
use std::env;
use tokio::sync::mpsc;

mod connectors;
mod core;
mod storage;
mod strategies;
mod tui;
mod types;
mod utils;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();

    // 1. Load Configuration
    let api_key = env::var("BINANCE_API_KEY").unwrap_or_default();
    let secret_key = env::var("BINANCE_SECRET_KEY").unwrap_or_default();

    // Parse LIVE_TRADING env var (default to false for safety)
    let live_trading = env::var("LIVE_TRADING")
        .unwrap_or("false".to_string())
        .parse::<bool>()
        .unwrap_or(false);

    let symbol = "BTCUSDT";

    println!("========================================");
    println!("       THE SNIPER BOT - v0.1.0");
    println!("========================================");
    println!("Target: {}", symbol);
    println!(
        "Mode:   {}",
        if live_trading {
            "🚨 LIVE TRADING"
        } else {
            "📝 PAPER TRADING"
        }
    );
    println!("========================================");

    // 2. Initialize Components
    let mut client = BinanceClient::new(api_key, secret_key);
    let strategy = SimpleScalper::new(symbol.to_string());

    // 3. Create Channels
    let (ticker_tx, ticker_rx) = mpsc::channel(100);

    // 4. Subscribe to Data
    // We clone the client for the engine (execution) vs the subscription (stream)
    // Note: If BinanceClient doesn't support Clone, we might need to split responsibilities.
    // Assuming we use the client for subscription here:
    client.subscribe_ticker(symbol, ticker_tx).await?;

    // 5. Run Engine
    // Pass the `live_trading` flag to the engine
    let mut engine = TradingEngine::new(client, strategy, ticker_rx, live_trading);

    if let Err(e) = engine.run().await {
        eprintln!("Fatal Engine Error: {}", e);
    }

    Ok(())
}
