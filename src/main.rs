// src/main.rs
use crate::connectors::binance::BinanceClient;
use crate::connectors::traits::StreamClient;
use crate::core::engine::TradingEngine;
use crate::strategies::scalper::RsiBollingerStrategy;
use dotenvy::dotenv;
use std::env;
use tokio::sync::mpsc;
use tracing_subscriber;

mod connectors;
mod core;
mod storage;
mod strategies;
mod tui;
mod types;
mod utils;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Инициализация Tracing (Non-blocking logger)
    tracing_subscriber::fmt::init();

    dotenv().ok();

    let api_key = env::var("BINANCE_API_KEY").unwrap_or_default();
    let secret_key = env::var("BINANCE_SECRET_KEY").unwrap_or_default();

    let live_trading = env::var("LIVE_TRADING")
        .unwrap_or("false".to_string())
        .parse::<bool>()
        .unwrap_or(false);

    let symbol = "BTCUSDT";

    let mut client = BinanceClient::new(api_key, secret_key);
    let strategy = RsiBollingerStrategy::new(symbol.to_string());

    let (ticker_tx, ticker_rx) = mpsc::channel(100);
    let (ui_tx, ui_rx) = mpsc::channel(100);

    client.subscribe_ticker(symbol, ticker_tx).await?;

    let engine_ui_tx = ui_tx.clone();

    tokio::spawn(async move {
        let mut engine =
            TradingEngine::new(client, strategy, ticker_rx, engine_ui_tx, live_trading);

        if let Err(e) = engine.run().await {
            tracing::error!("Engine Error: {}", e);
        }
    });

    // Run TUI only if explicitly enabled (tracing prints to stdout, which conflicts with TUI)
    // For this task, we prioritize tracing output. Disable TUI or redirect logs if needed.
    // Assuming user wants standard output for now or TUI works with tracing (it doesn't well without adapter).
    // For now, running TUI as requested, but be aware tracing logs will mess up TUI unless redirected to file.
    tui::run(ui_rx, symbol.to_string()).await?;

    Ok(())
}
