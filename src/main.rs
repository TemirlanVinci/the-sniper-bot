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

    let api_key = env::var("BINANCE_API_KEY").unwrap_or_default();
    let secret_key = env::var("BINANCE_SECRET_KEY").unwrap_or_default();

    let symbol = "BTCUSDT";

    // 1. Initialize Components
    let mut client = BinanceClient::new(api_key, secret_key);
    // Note: We skip client.connect() check to start TUI faster,
    // or you can await it and handle errors before TUI starts.

    let strategy = SimpleScalper::new(symbol.to_string());

    // 2. Create Channels
    // Channel for Market Data (Binance -> Engine)
    let (ticker_tx, ticker_rx) = mpsc::channel(100);
    // Channel for UI Events (Engine -> TUI)
    let (ui_tx, ui_rx) = mpsc::channel(100);

    // 3. Subscribe to Data
    // We clone the client or ensure StreamClient::subscribe_ticker works with &mut
    client.subscribe_ticker(symbol, ticker_tx).await?;

    // 4. Spawn Engine Task (Background)
    let engine_ui_tx = ui_tx.clone();
    tokio::spawn(async move {
        let mut engine = TradingEngine::new(client, strategy, ticker_rx, engine_ui_tx);
        if let Err(e) = engine.run().await {
            eprintln!("Engine Error: {}", e);
        }
    });

    // 5. Run TUI (Main Thread)
    // TUI blocks the main thread until the user presses 'q'
    tui::run(ui_rx, symbol.to_string()).await?;

    Ok(())
}
