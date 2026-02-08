use crate::connectors::binance::BinanceClient;
use crate::connectors::traits::{ExchangeClient, StreamClient};
use crate::strategies::scalper::SimpleScalper;
use crate::strategies::traits::Strategy;
use crate::types::Signal;
// ИСПРАВЛЕНИЕ ТУТ: используем dotenvy
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

    let mut client = BinanceClient::new(api_key, secret_key);

    println!("Connecting to Binance API...");
    match client.connect().await {
        Ok(_) => println!("✅ REST API Connected!"),
        Err(e) => {
            eprintln!("❌ Connection failed: {}", e);
            return Ok(());
        }
    }

    let (tx, mut rx) = mpsc::channel(100);

    println!("Subscribing to {} market data...", symbol);
    client.subscribe_ticker(symbol, tx).await?;

    let mut strategy = SimpleScalper::new(symbol.to_string(), 1.0, 0.5);
    strategy.init().await?;

    println!(">>> Bot started. Watching market... (Press Ctrl+C to stop)");

    while let Some(ticker) = rx.recv().await {
        println!("Tick: {} | ${:.2}", ticker.symbol, ticker.price);

        // ИСПРАВЛЕНИЕ: Мы вызываем логику прямо тут, как и договаривались.
        // Файл engine.rs пока не используется в main.rs, но компилятор его проверяет и ругается.
        // Чтобы починить main, этого достаточно.
        let signal = strategy.on_tick(&ticker).await?;

        match signal {
            Signal::Advice(side, price) => {
                println!("🔥 SIGNAL!!! {:?} at ${}", side, price);
            }
            Signal::Hold => {}
        }
    }

    Ok(())
}
