use crate::types::{Position, Signal, Ticker}; // Исправлен путь: types лежат в корне
use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc; // Добавлено

#[async_trait]
pub trait Strategy: Send + Sync {
    fn name(&self) -> String;

    // Инициализация стратегии (например, загрузка истории)
    async fn init(&mut self) -> Result<()>;

    // Обработка нового тика (цена изменилась)
    async fn on_tick(&mut self, ticker: &Ticker) -> Result<Signal>;

    // Возможность обновить состояние позиций (если биржа подтвердила ордер)
    fn update_position(&mut self, position: &Position);
}
