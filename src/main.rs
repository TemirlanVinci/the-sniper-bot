// src/main.rs
use crate::connectors::binance::BinanceClient;
use crate::connectors::traits::StreamClient; // Removed unused ExchangeClient
use crate::core::engine::TradingEngine;
use crate::strategies::scalper::RsiBollingerStrategy;
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

    // Parse LIVE_TRADING env var
    let live_trading = env::var("LIVE_TRADING")
        .unwrap_or("false".to_string())
        .parse::<bool>()
        .unwrap_or(false);

    let symbol = "BTCUSDT";

    // 1. Initialize Components
    let mut client = BinanceClient::new(api_key, secret_key);
    let strategy = RsiBollingerStrategy::new(symbol.to_string());

    // 2. Create Channels
    let (ticker_tx, ticker_rx) = mpsc::channel(100);
    let (ui_tx, ui_rx) = mpsc::channel(100);

    // 3. Subscribe to Data
    client.subscribe_ticker(symbol, ticker_tx).await?;

    // 4. Spawn Engine Task (Background)
    // We clone ui_tx so we can pass it to the engine
    let engine_ui_tx = ui_tx.clone();

    tokio::spawn(async move {
        // FIX: Pass all required arguments: exchange, strategy, rx, ui_sender, live_mode
        let mut engine =
            TradingEngine::new(client, strategy, ticker_rx, engine_ui_tx, live_trading);

        if let Err(e) = engine.run().await {
            eprintln!("Engine Error: {}", e);
        }
    });

    // 5. Run TUI (Main Thread)
    tui::run(ui_rx, symbol.to_string()).await?;

    Ok(())
}
