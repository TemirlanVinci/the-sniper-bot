use crate::connectors::traits::ExchangeClient;
use crate::strategies::traits::Strategy;
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
    E: ExchangeClient + Send,
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
            let signal = self.strategy.on_tick(&ticker).await?;

            match signal {
                Signal::Advice(side, price) => {
                    // Log the signal for Paper Trading simulation
                    println!(
                        "🔥 [SIMULATION] SIGNAL: {:?} at ${:.2} for {}",
                        side, price, ticker.symbol
                    );
                }
                Signal::Hold => {
                    // Log ticks periodically or keep silent for less noise
                    // println!("Tick: {} | ${:.2}", ticker.symbol, ticker.price);
                }
            }
        }

        Ok(())
    }
}
