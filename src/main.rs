mod connectors;
mod types;

use crate::connectors::binance::BinanceClient;
use crate::connectors::traits::{ExchangeClient, StreamClient};
use anyhow::{Context, Result};
use dotenvy::dotenv;
use std::env;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Load environment variables
    dotenv().ok();

    // Quick logger setup (optional, but good practice)
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    tracing_subscriber::fmt::init();

    println!("--- Starting Binance Connector Smoke Test ---");

    // 2. Retrieve Credentials
    let api_key = env::var("BINANCE_API_KEY").context("BINANCE_API_KEY must be set in .env")?;
    let secret_key =
        env::var("BINANCE_SECRET_KEY").context("BINANCE_SECRET_KEY must be set in .env")?;

    // 3. Instantiate Client
    let mut client = BinanceClient::new(api_key, secret_key);

    // 4. Test Connectivity (Ping)
    println!("\n[1/4] Testing Connectivity...");
    match client.connect().await {
        Ok(_) => println!("✅ Connection successful (Ping OK)"),
        Err(e) => {
            eprintln!("❌ Connection failed: {}", e);
            return Err(e);
        }
    }

    // 5. Test Private Endpoint (Balance) - Requires valid Signature
    println!("\n[2/4] Fetching USDT Balance...");
    match client.get_balance("USDT").await {
        Ok(balance) => println!("✅ USDT Balance: {:.2}", balance),
        Err(e) => eprintln!("❌ Failed to fetch balance: {}", e),
    }

    // 6. Test Public Endpoint (Price)
    println!("\n[3/4] Fetching BTCUSDT Price...");
    match client.fetch_price("BTCUSDT").await {
        Ok(ticker) => println!("✅ BTC Price: ${:.2}", ticker.price),
        Err(e) => eprintln!("❌ Failed to fetch price: {}", e),
    }

    // 7. Test WebSocket Stream
    println!("\n[4/4] Subscribing to BTCUSDT ticker stream...");
    // This spawns the background task defined in binance.rs
    if let Err(e) = client.subscribe_ticker("BTCUSDT").await {
        eprintln!("❌ Failed to subscribe: {}", e);
    } else {
        println!("✅ Subscription request sent. Listening for 10 seconds...");
    }

    // 8. Keep Alive to observe Stream
    println!("\n--- Watching Stream (Press Ctrl+C to stop early) ---");
    sleep(Duration::from_secs(10)).await;

    println!("\n--- Test Complete ---");
    Ok(())
}
