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

    let api_key = env::var("BINANCE_API_KEY").expect("BINANCE_API_KEY not set");
    let secret_key = env::var("BINANCE_SECRET_KEY").expect("BINANCE_SECRET_KEY not set");

    let symbol = "BTCUSDT";

    println!("--- Initializing Sniper Bot ---");

    // 1. Initialize Binance Client
    let mut client = BinanceClient::new(api_key, secret_key);
    client.connect().await?;
    println!("✅ REST API Connected!");

    // 2. Initialize Strategy
    let strategy = SimpleScalper::new(symbol.to_string());

    // 3. Create Channel for Tickers
    let (tx, rx) = mpsc::channel(100);

    // 4. Subscribe to Tickers (Spawns background task)
    println!("Subscribing to {} market data...", symbol);
    client.subscribe_ticker(symbol, tx).await?;

    // 5. Initialize and Run Engine
    let mut engine = TradingEngine::new(client, strategy, rx);

    println!(">>> Bot started. Watching market... (Press Ctrl+C to stop)");
    engine.run().await?;

    Ok(())
}
