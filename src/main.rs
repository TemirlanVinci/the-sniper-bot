mod connectors;
mod strategies;
mod types;
// mod engine; // Assuming you have this module

use crate::connectors::binance::BinanceClient;
use crate::strategies::simple_scalper::SimpleScalper;
use crate::types::Ticker;
// use crate::engine::TradingEngine; // Import your Engine struct here

use anyhow::{Context, Result};
use dotenvy::dotenv;
use futures_util::StreamExt;
use serde::Deserialize;
use std::env;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use url::Url;

// Helper struct to parse raw Binance Trade events
#[derive(Debug, Deserialize)]
struct BinanceTradeEvent {
    s: String, // Symbol
    p: String, // Price
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    tracing_subscriber::fmt::init();

    println!("--- Initializing Trading Bot ---");

    // 1. Configuration
    let api_key = env::var("BINANCE_API_KEY").context("Missing BINANCE_API_KEY")?;
    let secret_key = env::var("BINANCE_SECRET_KEY").context("Missing BINANCE_SECRET_KEY")?;
    let symbol = "BTCUSDT";

    // 2. Initialize Components
    let client = BinanceClient::new(api_key, secret_key);

    // Scalper: 0.1% drop to buy, 0.2% profit to sell
    let strategy = SimpleScalper::new(0.001, 0.002);

    // 3. Create Channels
    // The Engine will receive Tickers from this channel
    let (tx, rx) = mpsc::channel::<Ticker>(100);

    // 4. Initialize Engine
    // Assumption: TradingEngine::new(strategy, exchange_client, ticker_receiver)
    // Note: We wrap the client in a Box or Arc if the Engine requires shared ownership/polymorphism.
    // For this example, we assume the Engine takes ownership or a reference.
    // let mut engine = TradingEngine::new(strategy, client, rx);

    println!(">>> Spawning Market Data Stream for {}", symbol);

    // 5. Spawn WebSocket Task (The "Driver")
    // We do this manually here to pipe data into 'tx'
    let connect_url = format!(
        "wss://stream.binance.com:9443/ws/{}@trade",
        symbol.to_lowercase()
    );
    let url = Url::parse(&connect_url)?;

    tokio::spawn(async move {
        match connect_async(url).await {
            Ok((ws_stream, _)) => {
                println!("✅ WebSocket connected.");
                let (_, mut read) = ws_stream.split();

                while let Some(message) = read.next().await {
                    match message {
                        Ok(msg) => {
                            if let Ok(text) = msg.to_text() {
                                // Parse the Binance specific JSON
                                match serde_json::from_str::<BinanceTradeEvent>(text) {
                                    Ok(event) => {
                                        // Convert to our generic Ticker
                                        let ticker = Ticker {
                                            symbol: event.s,
                                            price: event.p.parse().unwrap_or(0.0),
                                        };

                                        // Send to Engine
                                        if let Err(e) = tx.send(ticker).await {
                                            eprintln!("❌ Failed to send ticker to Engine: {}", e);
                                            break; // Stop if receiver is dropped
                                        }
                                    }
                                    Err(e) => eprintln!("Failed to parse trade event: {}", e),
                                }
                            }
                        }
                        Err(e) => eprintln!("WebSocket error: {}", e),
                    }
                }
            }
            Err(e) => eprintln!("Failed to connect to WebSocket: {}", e),
        }
        println!("⚠️ WebSocket task terminated.");
    });

    println!(">>> Running Trading Engine...");

    // 6. Run the Engine
    // engine.run().await?;

    // Placeholder to keep main alive if Engine is not yet implemented:
    println!("(Engine placeholder: Listening for data...)");
    tokio::signal::ctrl_c().await?;

    Ok(())
}
