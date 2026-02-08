// src/strategies/scalper.rs
use crate::strategies::traits::Strategy;
use crate::types::{Position, Side, Signal, Ticker};
use anyhow::Result;
use async_trait::async_trait;

pub struct SimpleScalper {
    symbol: String,
    initial_price: Option<f64>,
    position: Option<Position>,
}

impl SimpleScalper {
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            initial_price: None,
            position: None,
        }
    }
}

#[async_trait]
impl Strategy for SimpleScalper {
    fn name(&self) -> String {
        "SimpleScalper".to_string()
    }

    async fn init(&mut self) -> Result<()> {
        println!("Strategy {} initialized for {}", self.name(), self.symbol);
        Ok(())
    }

    async fn on_tick(&mut self, ticker: &Ticker) -> Result<Signal> {
        // 1. Manage Base Price (Anchor)
        // If we just finished a trade or started fresh (initial_price is None),
        // reset anchor to current market price.
        let base_price = match self.initial_price {
            Some(p) => p,
            None => {
                self.initial_price = Some(ticker.price);
                println!("⚓ Base price anchored at: ${:.2}", ticker.price);
                return Ok(Signal::Hold);
            }
        };

        // 2. Define Thresholds
        // Buy: 0.5% drop from base
        let buy_threshold = base_price * 0.995;
        // Take Profit: 0.5% rise from base (approx 1% gain from entry)
        let take_profit_threshold = base_price * 1.005;
        // Stop Loss: 1.5% drop from base (approx 1% loss from entry)
        // CRITICAL: Protects capital if market dumps
        let stop_loss_threshold = base_price * 0.985;

        // 3. Determine Action
        match &self.position {
            // State: LOOKING FOR ENTRY (No Position)
            None => {
                if ticker.price < buy_threshold {
                    println!(
                        "📉 Dip detected (${:.2} < ${:.2}) -> BUY SIGNAL",
                        ticker.price, buy_threshold
                    );
                    return Ok(Signal::Advice(Side::Buy, ticker.price));
                }
            }
            // State: LOOKING FOR EXIT (Has Position)
            Some(_pos) => {
                if ticker.price > take_profit_threshold {
                    println!(
                        "📈 Target hit (${:.2} > ${:.2}) -> TAKE PROFIT",
                        ticker.price, take_profit_threshold
                    );
                    return Ok(Signal::Advice(Side::Sell, ticker.price));
                } else if ticker.price < stop_loss_threshold {
                    println!(
                        "🛑 Stop Loss hit (${:.2} < ${:.2}) -> STOP LOSS",
                        ticker.price, stop_loss_threshold
                    );
                    return Ok(Signal::Advice(Side::Sell, ticker.price));
                }
            }
        }

        Ok(Signal::Hold)
    }

    fn update_position(&mut self, position: Option<Position>) {
        if let Some(ref pos) = position {
            println!("✅ Position OPENED at ${:.2}", pos.entry_price);
        } else {
            println!("❎ Position CLOSED. Resetting cycle...");
            // CRITICAL LOGIC FIX:
            // Reset initial_price to None. This forces on_tick to
            // grab the NEW current price as the base for the next trade cycle.
            self.initial_price = None;
        }
        self.position = position;
    }
}
