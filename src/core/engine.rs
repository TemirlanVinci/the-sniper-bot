// src/core/engine.rs
use crate::connectors::traits::ExchangeClient;
use crate::strategies::traits::Strategy;
use crate::types::{Signal, Ticker, UiEvent};
use anyhow::Result;
use tokio::sync::mpsc;

pub struct TradingEngine<E, S> {
    exchange: E,
    strategy: S,
    ticker_receiver: mpsc::Receiver<Ticker>,
    ui_sender: mpsc::Sender<UiEvent>,
}

impl<E, S> TradingEngine<E, S>
where
    E: ExchangeClient + Send,
    S: Strategy,
{
    pub fn new(
        exchange: E,
        strategy: S,
        ticker_receiver: mpsc::Receiver<Ticker>,
        ui_sender: mpsc::Sender<UiEvent>,
    ) -> Self {
        Self {
            exchange,
            strategy,
            ticker_receiver,
            ui_sender,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        let _ = self
            .ui_sender
            .send(UiEvent::Log("Engine started".into()))
            .await;
        self.strategy.init().await?;

        while let Some(ticker) = self.ticker_receiver.recv().await {
            // 1. Send Ticker Update to UI
            let _ = self
                .ui_sender
                .send(UiEvent::TickerUpdate(ticker.clone()))
                .await;

            // 2. Process Strategy
            let signal = self.strategy.on_tick(&ticker).await?;

            match signal {
                Signal::Advice(side, price) => {
                    // Send Signal to UI
                    let _ = self.ui_sender.send(UiEvent::Signal(signal.clone())).await;
                    let _ = self
                        .ui_sender
                        .send(UiEvent::Log(format!("EXECUTING {:?} at {}", side, price)))
                        .await;

                    // Here we would call self.exchange.place_order(...)
                }
                Signal::Hold => {
                    // No action
                }
            }
        }

        Ok(())
    }
}
