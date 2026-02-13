// src/main.rs
use crate::connectors::binance::BinanceClient;
use crate::connectors::traits::StreamClient;
use crate::core::engine::TradingEngine;
use crate::strategies::scalper::RsiBollingerStrategy;
use dotenvy::dotenv;
use std::env;
use tokio::sync::mpsc;
use tracing_appender::rolling;
use tracing_subscriber::fmt::writer::MakeWriterExt;

mod connectors;
mod core;
mod strategies;
mod tui;
mod types;
mod utils;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();

    // --- 1. Async Non-blocking Logging ---
    // Логи пишутся в файлы: logs/sniper.log.YYYY-MM-DD
    let file_appender = rolling::daily("logs", "sniper.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // Дублируем логи: в файл и в stderr (если не TUI)
    // Так как у нас TUI, лучше писать ТОЛЬКО в файл, чтобы не ломать интерфейс.
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false) // Отключаем цвета для файла
        .init();

    tracing::info!("Starting The Sniper Bot (Refactored Core)...");

    let api_key = env::var("BINANCE_API_KEY").unwrap_or_default();
    let secret_key = env::var("BINANCE_SECRET_KEY").unwrap_or_default();
    let live_trading = env::var("LIVE_TRADING")
        .unwrap_or("false".to_string())
        .parse::<bool>()
        .unwrap_or(false);

    let symbol = "BTCUSDT";

    // --- 2. Setup Clients & Channels ---
    let mut binance_client = BinanceClient::new(api_key, secret_key);
    // Клонируем клиент, так как он теперь ExecutionHandler + StreamClient
    let execution_client = Box::new(binance_client.clone());

    let strategy = RsiBollingerStrategy::new(symbol.to_string());

    // Bounded Channel (Backpressure) - Capacity 100 ticks
    let (ticker_tx, ticker_rx) = mpsc::channel(100);
    // UI Channel
    let (ui_tx, ui_rx) = mpsc::channel(100);

    // --- 3. Start WebSocket Stream ---
    binance_client.subscribe_ticker(symbol, ticker_tx).await?;

    // --- 4. Start Engine Actor ---
    let engine_ui_tx = ui_tx.clone();
    tokio::spawn(async move {
        let mut engine = TradingEngine::new(
            execution_client,
            strategy,
            ticker_rx,
            engine_ui_tx,
            live_trading,
        );

        if let Err(e) = engine.run().await {
            tracing::error!("FATAL Engine Error: {}", e);
        }
    });

    // --- 5. Start TUI ---
    // TUI работает в основном потоке
    tui::run(ui_rx, symbol.to_string()).await?;

    Ok(())
}
