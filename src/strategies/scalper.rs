use crate::strategies::traits::{Signal, Strategy};
use crate::types::{Inventory, Side, Ticker}; // Assuming Inventory is defined in types
use anyhow::Result;

pub struct SimpleScalper {
    target_drop_pct: f64,
    target_profit_pct: f64,
    initial_price: Option<f64>,
}

impl SimpleScalper {
    /// Creates a new SimpleScalper strategy.
    ///
    /// # Arguments
    /// * `target_drop_pct` - The percentage drop from initial price to trigger a BUY (e.g., 0.02 for 2%).
    /// * `target_profit_pct` - The percentage gain from entry price to trigger a SELL (e.g., 0.03 for 3%).
    pub fn new(target_drop_pct: f64, target_profit_pct: f64) -> Self {
        Self {
            target_drop_pct,
            target_profit_pct,
            initial_price: None,
        }
    }
}

impl Strategy for SimpleScalper {
    fn process(&mut self, ticker: &Ticker, inventory: &Inventory) -> Signal {
        // 1. Initialize Baseline on First Tick
        if self.initial_price.is_none() {
            println!("Strategy: Initializing baseline price at {}", ticker.price);
            self.initial_price = Some(ticker.price);
            return Signal::None;
        }

        let initial_price = self.initial_price.unwrap();

        // 2. Logic: No Position -> Look for Dip (Buy)
        if inventory.crypto == 0.0 {
            let buy_target = initial_price * (1.0 - self.target_drop_pct);

            if ticker.price <= buy_target {
                println!(
                    "Strategy: Price {} is below target {} (Drop: {}%). Signal: BUY",
                    ticker.price,
                    buy_target,
                    self.target_drop_pct * 100.0
                );
                return Signal::Advice(Side::Buy);
            }
        }
        // 3. Logic: Have Position -> Look for Profit (Sell)
        else {
            // We rely on inventory to tell us our entry price (average cost basis)
            let sell_target = inventory.average_cost * (1.0 + self.target_profit_pct);

            if ticker.price >= sell_target {
                println!(
                    "Strategy: Price {} met profit target {} (Gain: {}%). Signal: SELL",
                    ticker.price,
                    sell_target,
                    self.target_profit_pct * 100.0
                );
                return Signal::Advice(Side::Sell);
            }
        }

        Signal::None
    }
}
