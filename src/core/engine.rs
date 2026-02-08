use crate::connectors::traits::ExchangeClient;
use crate::strategies::traits::Strategy;
// ИСПРАВЛЕНИЕ 1: Правильный путь к типам
use crate::types::{Signal, Ticker};
use anyhow::Result;
use tokio::sync::mpsc;

pub struct TradingEngine<E, S> {
    exchange: E,
    strategy: S,
    ticker_receiver: mpsc::Receiver<Ticker>,
}

impl<E, S> TradingEngine<E, S>
where
    E: ExchangeClient,
    S: Strategy,
{
    pub fn new(exchange: E, strategy: S, ticker_receiver: mpsc::Receiver<Ticker>) -> Self {
        Self {
            exchange,
            strategy,
            ticker_receiver,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        println!("Starting Trading Engine...");
        self.strategy.init().await?;

        while let Some(ticker) = self.ticker_receiver.recv().await {
            // ИСПРАВЛЕНИЕ 2: Заменили .process() на .on_tick()
            let signal = self.strategy.on_tick(&ticker).await?;

            match signal {
                Signal::Advice(side, price) => {
                    println!("Engine received advice: {:?} at {}", side, price);
                    // В будущем тут будет вызов: self.exchange.place_order(...)
                }
                Signal::Hold => {
                    // Holding...
                }
            }
        }

        Ok(())
    }
}
