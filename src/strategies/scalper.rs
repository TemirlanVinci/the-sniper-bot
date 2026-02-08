use crate::strategies::traits::Strategy;
use crate::types::{Position, Side, Signal, Ticker}; // Берем типы напрямую из types.rs
use anyhow::Result;

pub struct SimpleScalper {
    symbol: String,
    target_profit_percent: f64,
    stop_loss_percent: f64,
    last_price: f64,
    position: Option<Position>,
}

impl SimpleScalper {
    pub fn new(symbol: String, target_profit: f64, stop_loss: f64) -> Self {
        Self {
            symbol,
            target_profit_percent: target_profit,
            stop_loss_percent: stop_loss,
            last_price: 0.0,
            position: None,
        }
    }
}

#[async_trait::async_trait]
impl Strategy for SimpleScalper {
    fn name(&self) -> String {
        "SimpleScalper".to_string()
    }

    async fn init(&mut self) -> Result<()> {
        println!("Strategy {} initialized for {}", self.name(), self.symbol);
        Ok(())
    }

    async fn on_tick(&mut self, ticker: &Ticker) -> Result<Signal> {
        self.last_price = ticker.price;

        // Простейшая логика (заглушка):
        // Если нет позиции -> покупаем (Signal::Advice)
        // Если есть позиция -> держим (Signal::Hold)

        // В реальном коде тут будет математика
        if self.position.is_none() {
            // Пример сигнала на покупку
            // return Ok(Signal::Advice(Side::Buy, ticker.price));
            return Ok(Signal::Hold);
        }

        Ok(Signal::Hold)
    }

    fn update_position(&mut self, position: &Position) {
        self.position = Some(position.clone());
    }
}
