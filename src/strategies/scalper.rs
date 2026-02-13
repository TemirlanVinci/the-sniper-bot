// src/strategies/scalper.rs
use crate::strategies::traits::Strategy;
use crate::types::{Position, Side, Signal, Ticker};
use anyhow::Result;
use async_trait::async_trait;
use ta::indicators::{BollingerBands, RelativeStrengthIndex};
use ta::{DataItem, Next};

/// Структура для накопления данных "виртуальной" минутной свечи
#[derive(Debug, Clone)]
struct CandleBuilder {
    open_time: u64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
}

impl CandleBuilder {
    fn new(tick: &Ticker) -> Self {
        // Округляем до начала минуты (в миллисекундах)
        let open_time = (tick.timestamp / 60_000) * 60_000;
        Self {
            open_time,
            open: tick.price,
            high: tick.price,
            low: tick.price,
            close: tick.price,
        }
    }

    fn update(&mut self, tick: &Ticker) {
        if tick.price > self.high {
            self.high = tick.price;
        }
        if tick.price < self.low {
            self.low = tick.price;
        }
        self.close = tick.price;
    }
}

pub struct RsiBollingerStrategy {
    symbol: String,
    // Индикаторы из крейта `ta`
    rsi: RelativeStrengthIndex,
    bb: BollingerBands,
    // Состояние свечи
    current_candle: Option<CandleBuilder>,
    // Последние рассчитанные значения индикаторов (для проверки условий на каждом тике)
    last_rsi_value: f64,
    last_bb_values: Option<(f64, f64, f64)>, // (Lower, Middle, Upper)
    // Текущая позиция
    position: Option<Position>,
}

impl RsiBollingerStrategy {
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            // RSI период 14
            rsi: RelativeStrengthIndex::new(14).unwrap(),
            // Bollinger Bands: период 20, стандартное отклонение 2.0
            bb: BollingerBands::new(20, 2.0).unwrap(),
            current_candle: None,
            last_rsi_value: 50.0, // Нейтральное начальное значение
            last_bb_values: None,
            position: None,
        }
    }

    /// Обработка закрытия свечи и обновление индикаторов
    fn close_candle(&mut self, candle: &CandleBuilder) {
        let item = DataItem::builder()
            .high(candle.high)
            .low(candle.low)
            .close(candle.close)
            .open(candle.open)
            .volume(0.0) // Объем нам не критичен для RSI/BB, но нужен для DataItem
            .build()
            .unwrap();

        // Скармливаем свечу индикаторам
        self.last_rsi_value = self.rsi.next(&item);
        let bb_out = self.bb.next(&item);
        self.last_bb_values = Some((bb_out.lower, bb_out.average, bb_out.upper));

        println!(
            "🕯 Candle Closed [{}]: Close=${:.2} | RSI={:.2} | BB_Low={:.2} BB_Mid={:.2}",
            self.symbol, candle.close, self.last_rsi_value, bb_out.lower, bb_out.average
        );
    }
}

#[async_trait]
impl Strategy for RsiBollingerStrategy {
    fn name(&self) -> String {
        "RsiBollingerScalper".to_string()
    }

    async fn init(&mut self) -> Result<()> {
        println!(
            "🚀 Strategy {} initialized for {}",
            self.name(),
            self.symbol
        );
        Ok(())
    }

    async fn on_tick(&mut self, tick: &Ticker) -> Result<Signal> {
        // 1. Управление свечами (Tick Aggregation)
        let tick_minute_start = (tick.timestamp / 60_000) * 60_000;

        match self.current_candle.clone() {
            Some(mut candle) => {
                if tick_minute_start > candle.open_time {
                    // Минута сменилась -> закрываем старую свечу
                    self.close_candle(&candle);
                    // Начинаем новую свечу
                    self.current_candle = Some(CandleBuilder::new(tick));
                } else {
                    // Та же минута -> обновляем текущую свечу
                    candle.update(tick);
                    self.current_candle = Some(candle);
                }
            }
            None => {
                // Первая свеча
                self.current_candle = Some(CandleBuilder::new(tick));
            }
        }

        // Если индикаторы еще не рассчитаны (нет закрытых свечей), ждем
        let (bb_lower, bb_mid, _bb_upper) = match self.last_bb_values {
            Some(vals) => vals,
            None => return Ok(Signal::Hold),
        };

        // 2. Торговая логика
        match &self.position {
            // --- ЛОГИКА ВХОДА (LONG) ---
            None => {
                // Условия:
                // 1. Цена пробила НИЖНЮЮ полосу (Price < Lower Band)
                // 2. RSI в зоне перепроданности (RSI < 30)
                if tick.price < bb_lower && self.last_rsi_value < 30.0 {
                    println!(
                        "⚡ SIGNAL BUY: Price {:.2} < BB_Low {:.2} AND RSI {:.2} < 30",
                        tick.price, bb_lower, self.last_rsi_value
                    );
                    return Ok(Signal::Advice(Side::Buy, tick.price));
                }
            }

            // --- ЛОГИКА ВЫХОДА (EXIT) ---
            Some(pos) => {
                // 1. Жесткий стоп-лосс (-1.0%)
                let stop_loss_price = pos.entry_price * 0.99;
                if tick.price <= stop_loss_price {
                    println!(
                        "🛑 STOP LOSS TRIGGERED: {:.2} <= {:.2}",
                        tick.price, stop_loss_price
                    );
                    return Ok(Signal::Advice(Side::Sell, tick.price));
                }

                // 2. Тейк-профит
                // Условия: Цена коснулась СРЕДНЕЙ полосы ИЛИ RSI > 50
                let tp_condition_1 = tick.price >= bb_mid;
                let tp_condition_2 = self.last_rsi_value > 50.0;

                if tp_condition_1 || tp_condition_2 {
                    println!(
                        "💰 TAKE PROFIT: Price {:.2} >= BB_Mid {:.2} OR RSI {:.2} > 50",
                        tick.price, bb_mid, self.last_rsi_value
                    );
                    return Ok(Signal::Advice(Side::Sell, tick.price));
                }
            }
        }

        Ok(Signal::Hold)
    }

    fn update_position(&mut self, position: Option<Position>) {
        if let Some(ref pos) = position {
            println!(
                "✅ Position OPENED: {} @ ${:.2}",
                pos.symbol, pos.entry_price
            );
        } else {
            println!("❎ Position CLOSED");
        }
        self.position = position;
    }
}
