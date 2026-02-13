// src/core/engine.rs
use crate::config::AppConfig;
use crate::connectors::traits::ExecutionHandler;
use crate::strategies::traits::Strategy;
use crate::types::{Position, Side, Signal, Ticker, UiEvent};
use crate::utils::precision::{normalize_price, normalize_quantity}; // Импорт утилит
use anyhow::Result;
use rust_decimal::prelude::{FromPrimitive, ToPrimitive}; // Добавлен ToPrimitive для логирования если нужно
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

#[derive(Debug, Default, Serialize, Deserialize)]
struct EngineState {
    active_position: Option<Position>,
}

pub struct TradingEngine<S> {
    config: AppConfig,
    execution_handler: Box<dyn ExecutionHandler>,
    strategy: S,
    ticker_receiver: mpsc::Receiver<Ticker>,
    ui_sender: mpsc::Sender<UiEvent>,
    live_mode: bool,
    state_file: String,
}

impl<S> TradingEngine<S>
where
    S: Strategy,
{
    pub fn new(
        config: AppConfig,
        execution_handler: Box<dyn ExecutionHandler>,
        strategy: S,
        ticker_receiver: mpsc::Receiver<Ticker>,
        ui_sender: mpsc::Sender<UiEvent>,
        live_mode: bool,
    ) -> Self {
        Self {
            config,
            execution_handler,
            strategy,
            ticker_receiver,
            ui_sender,
            live_mode,
            state_file: "bot_state.json".to_string(),
        }
    }

    async fn load_state(&mut self) {
        if let Ok(data) = tokio::fs::read_to_string(&self.state_file).await {
            if let Ok(state) = serde_json::from_str::<EngineState>(&data) {
                info!("Restored state: {:?}", state);
                self.strategy.update_position(state.active_position);
            }
        }
    }

    async fn save_state(&self, position: Option<Position>) {
        let state = EngineState {
            active_position: position,
        };
        if let Ok(data) = serde_json::to_string_pretty(&state) {
            if let Err(e) = tokio::fs::write(&self.state_file, data).await {
                error!("Failed to save bot state: {}", e);
            }
        }
    }

    fn send_ui_event(&self, event: UiEvent) {
        match self.ui_sender.try_send(event) {
            Ok(_) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {}
            Err(mpsc::error::TrySendError::Closed(_)) => {
                error!("UI Channel closed! Interface is likely dead.");
            }
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        info!("Engine starting...");
        self.load_state().await;
        self.strategy.init().await?;

        info!("Engine loop running. Live Mode: {}", self.live_mode);

        while let Some(ticker) = self.ticker_receiver.recv().await {
            self.send_ui_event(UiEvent::TickerUpdate(ticker.clone()));

            let signal = self.strategy.on_tick(&ticker).await?;

            match signal {
                Signal::Advice(side, price) => {
                    self.handle_signal(side, price, &ticker).await?;
                }
                Signal::StateChanged => {
                    let current_pos = self.strategy.get_position();
                    self.save_state(current_pos).await;
                    info!("💾 State updated (highest_price tracked)");
                }
                Signal::Hold => {}
            }
        }
        Ok(())
    }

    async fn handle_signal(
        &mut self,
        side: Side,
        current_price: Decimal,
        ticker: &Ticker,
    ) -> Result<()> {
        info!("Signal detected: {:?} @ {}", side, current_price);
        self.send_ui_event(UiEvent::Signal(Signal::Advice(side, current_price)));

        // 1. Расчет "сырого" объема
        let order_usdt =
            Decimal::from_f64(self.config.order_size_usdt).unwrap_or(Decimal::from(10));
        let raw_qty = order_usdt / current_price;

        // 2. Нормализация объема (используем шаг из конфига)
        let step_size = self.config.symbol_step_size;
        let quantity = normalize_quantity(raw_qty, step_size);

        // 3. Проверка Min Notional (>$5.5)
        let notional_value = quantity * current_price;
        let min_notional = Decimal::from_str("5.5").unwrap(); // Безопасный парсинг без макроса dec!

        if notional_value < min_notional {
            warn!(
                "Order skipped: Notional value ${:.2} < ${} (Min limit). Raw Qty: {}, Norm Qty: {}",
                notional_value, min_notional, raw_qty, quantity
            );
            return Ok(());
        }

        if quantity.is_zero() {
            warn!("⚠️ Quantity is zero after normalization. Not entering position.");
            return Ok(());
        }

        // 4. Подготовка цены (для лимитных ордеров или симуляции)
        // Для простоты берем tick_size из конфига
        let tick_size = self.config.symbol_tick_size;

        // В Paper Mode мы "исполняем" по текущей цене (или с проскальзыванием), но нормализуем её
        // В Live Mode ExecutionHandler сам может добавить slippage, но нам нужна базовая цена
        let target_price = normalize_price(current_price, tick_size);

        if !self.live_mode {
            // --- PAPER MODE ---
            let fake_pos = match side {
                Side::Buy => {
                    info!(
                        "Paper Buy: {} coins at ${} (Notional: ${:.2})",
                        quantity, target_price, notional_value
                    );

                    Some(Position {
                        symbol: ticker.symbol.clone(),
                        quantity,
                        entry_price: target_price,
                        unrealized_pnl: Decimal::ZERO,
                        highest_price: target_price,
                    })
                }
                Side::Sell => {
                    info!("Paper Sell: Closing position at ${}", target_price);
                    None
                }
            };
            self.strategy.update_position(fake_pos.clone());
            self.save_state(fake_pos).await;
            return Ok(());
        }

        // --- LIVE MODE ---
        // Для Live режима мы передаем нормализованное количество.
        // Цену execution_handler может пересчитать (slippage), но мы передадим ему "чистую"
        // или позволим ему самому решать. В текущей реализации handler принимает option price.

        // Добавим проскальзывание для лимитного ордера, чтобы он сработал как маркет (taker)
        // или оставим current_price если это Market Order (в зависимости от реализации handler).
        // Предположим, мы шлем Limit ордер с агрессивной ценой.

        let slippage_pct = Decimal::from_str("0.001").unwrap(); // 0.1%
        let execution_price_raw = match side {
            Side::Buy => current_price * (Decimal::ONE + slippage_pct),
            Side::Sell => current_price * (Decimal::ONE - slippage_pct),
        };
        let final_price = normalize_price(execution_price_raw, tick_size);

        info!(
            "Executing LIVE {:?}: Qty: {} @ Price: {} (Notional: ${:.2})",
            side, quantity, final_price, notional_value
        );

        match self
            .execution_handler
            .place_order(&ticker.symbol, side, quantity, Some(final_price))
            .await
        {
            Ok(order) => {
                info!("✅ Order Confirmed & Filled: {:?}", order);
                match side {
                    Side::Buy => {
                        let pos = Position {
                            symbol: ticker.symbol.clone(),
                            quantity,
                            entry_price: final_price, // В идеале брать из ответа биржи (avg_price)
                            unrealized_pnl: Decimal::ZERO,
                            highest_price: final_price,
                        };
                        self.strategy.update_position(Some(pos.clone()));
                        self.save_state(Some(pos)).await;
                    }
                    Side::Sell => {
                        self.strategy.update_position(None);
                        self.save_state(None).await;
                    }
                }
            }
            Err(e) => {
                error!("⚠️ Execution Error: {}", e);
            }
        }

        Ok(())
    }
}
