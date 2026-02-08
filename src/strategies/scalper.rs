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
        // Set initial price on the first tick
        let base_price = match self.initial_price {
            Some(p) => p,
            None => {
                self.initial_price = Some(ticker.price);
                println!("Initial price set to: ${:.2}", ticker.price);
                return Ok(Signal::Hold);
            }
        };

        // Determine action based on current position state
        match &self.position {
            // If we have no position, check for ENTRY (Buy)
            None => {
                // Logic: Buy if price drops below 99.5% of base
                if ticker.price < base_price * 0.995 {
                    return Ok(Signal::Advice(Side::Buy, ticker.price));
                }
            }
            // If we have a position, check for EXIT (Sell)
            Some(_pos) => {
                // Logic: Sell if price rises above 100.5% of base
                if ticker.price > base_price * 1.005 {
                    return Ok(Signal::Advice(Side::Sell, ticker.price));
                }
            }
        }

        Ok(Signal::Hold)
    }

    fn update_position(&mut self, position: Option<Position>) {
        if position.is_some() {
            println!("Strategy received confirmation: Position OPENED");
        } else {
            println!("Strategy received confirmation: Position CLOSED");
        }
        self.position = position;
    }
}
