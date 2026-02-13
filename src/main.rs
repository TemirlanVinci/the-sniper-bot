mod config;
mod connectors;
mod core;
mod strategies;
mod tui;
mod types;
mod utils;

use crate::config::AppConfig;
use crate::connectors::binance::BinanceClient;
use crate::connectors::traits::StreamClient;
use crate::core::engine::TradingEngine;
use crate::strategies::scalper::RsiBollingerStrategy;
use tokio::signal;
use tokio::sync::mpsc;
use tracing::{error, info};
use tracing_appender::rolling;
use tracing_subscriber::fmt::writer::MakeWriterExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Загружаем .env файл
    dotenvy::dotenv().ok();

    // 2. Настраиваем логи
    let file_appender = rolling::daily("logs", "bot.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .init();

    // 3. Загружаем конфиг
    let config = AppConfig::new()
        .expect("❌ Ошибка: Не удалось загрузить конфиг! Проверь Settings.toml и .env");

    info!("🚀 Starting Sniper Bot with Symbol: {}", config.symbol);

    // 4. Инициализация компонентов
    let mut binance_client = BinanceClient::new(config.api_key.clone(), config.secret_key.clone());

    // Fetch dynamic exchange info (Precision/StepSize)
    if let Err(e) = binance_client.fetch_exchange_info(&config.symbol).await {
        error!("⚠️ Failed to fetch exchange info: {}", e);
    }

    // Применяем настройки плеча
    if let Err(e) = binance_client
        .init_futures_settings(&config.symbol, config.leverage)
        .await
    {
        error!("⚠️ Failed to set leverage: {}", e);
    }

    let strategy = RsiBollingerStrategy::new(config.symbol.clone(), config.strategy.clone());
    let execution_handler = Box::new(binance_client.clone());

    // Каналы связи
    let (ticker_tx, ticker_rx) = mpsc::channel(100);
    let (ui_tx, ui_rx) = mpsc::channel(100);

    // 5. Запуск потока данных (WebSocket)
    binance_client
        .subscribe_ticker(&config.symbol, ticker_tx)
        .await?;

    // 6. Запуск движка (в фоне)
    // We clone config here to pass it into the engine
    let engine_config = config.clone();

    let engine_handle = tokio::spawn(async move {
        let mut engine = TradingEngine::new(
            engine_config, // <--- ADDED: Config is now the first argument
            execution_handler,
            strategy,
            ticker_rx,
            ui_tx,
            false, // Live Mode
        );
        if let Err(e) = engine.run().await {
            error!("❌ Engine CRITICAL error: {}", e);
        }
    });

    // 7. Обработка выхода (Ctrl+C)
    tokio::spawn(async move {
        signal::ctrl_c().await.unwrap();
        info!("🛑 Shutdown signal received.");
        std::process::exit(0);
    });

    // 8. Запуск TUI (Интерфейс)
    let app = tui::App::new(ui_rx, config.symbol.clone());
    if let Err(e) = app.run().await {
        eprintln!("TUI Error: {}", e);
    }

    let _ = engine_handle.await;
    Ok(())
}
