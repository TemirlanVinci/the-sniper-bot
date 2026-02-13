use crate::config::StrategyConfig; // <--- Добавили импорт конфига
use crate::strategies::traits::Strategy;
use crate::types::{Position, Side, Signal, Ticker};
use anyhow::Result;
use async_trait::async_trait;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use ta::indicators::{BollingerBands, RelativeStrengthIndex};
use ta::{DataItem, Next};
use tracing::info;

#[derive(Debug, Clone)]
struct CandleBuilder {
    open_time: u64,
    open: Decimal,
    high: Decimal,
    low: Decimal,
    close: Decimal,
}

impl CandleBuilder {
    fn new(tick: &Ticker) -> Self {
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
    rsi: RelativeStrengthIndex,
    bb: BollingerBands,
    current_candle: Option<CandleBuilder>,
    last_rsi_value: f64,
    last_bb_values: Option<(f64, f64, f64)>,
    position: Option<Position>,

    // Параметры
    obi_threshold: Decimal,
    trailing_callback: Decimal,
}

impl RsiBollingerStrategy {
    // ОБНОВЛЕНО: Добавили аргумент config: StrategyConfig
    pub fn new(symbol: String, config: StrategyConfig) -> Self {
        Self {
            symbol,
            // Используем настройки из конфига
            rsi: RelativeStrengthIndex::new(config.rsi_period).unwrap(),
            bb: BollingerBands::new(config.bb_period, config.bb_std_dev).unwrap(),

            current_candle: None,
            last_rsi_value: 50.0,
            last_bb_values: None,
            position: None,

            // Конвертируем f64 из конфига в Decimal
            obi_threshold: Decimal::from_f64(config.obi_threshold).unwrap_or(Decimal::ZERO),

            // Этот параметр пока оставил хардкодом (0.2%), можно тоже вынести в конфиг позже
            trailing_callback: Decimal::from_str("0.002").unwrap(),
        }
    }

    fn close_candle(&mut self, candle: &CandleBuilder) {
        let item = DataItem::builder()
            .high(candle.high.to_f64().unwrap_or_default())
            .low(candle.low.to_f64().unwrap_or_default())
            .close(candle.close.to_f64().unwrap_or_default())
            .open(candle.open.to_f64().unwrap_or_default())
            .volume(0.0)
            .build()
            .unwrap();

        self.last_rsi_value = self.rsi.next(&item);
        let bb_out = self.bb.next(&item);
        self.last_bb_values = Some((bb_out.lower, bb_out.average, bb_out.upper));
    }
}

#[async_trait]
impl Strategy for RsiBollingerStrategy {
    fn name(&self) -> String {
        "Fut_OBI_Scalper".to_string()
    }

    async fn init(&mut self) -> Result<()> {
        info!(
            "🚀 Strategy {} initialized with OBI Thresh: {}",
            self.name(),
            self.obi_threshold
        );
        Ok(())
    }

    async fn on_tick(&mut self, tick: &Ticker) -> Result<Signal> {
        // 1. Candle Logic
        let tick_minute_start = (tick.timestamp / 60_000) * 60_000;
        match self.current_candle.clone() {
            Some(mut candle) => {
                if tick_minute_start > candle.open_time {
                    self.close_candle(&candle);
                    self.current_candle = Some(CandleBuilder::new(tick));
                } else {
                    candle.update(tick);
                    self.current_candle = Some(candle);
                }
            }
            None => {
                self.current_candle = Some(CandleBuilder::new(tick));
            }
        }

        let (bb_lower_f, _bb_mid_f, _bb_upper_f) = match self.last_bb_values {
            Some(vals) => vals,
            None => return Ok(Signal::Hold),
        };
        let bb_lower = Decimal::from_f64(bb_lower_f).unwrap_or_default();

        // 2. OBI Calculation
        let total_qty = tick.bid_qty + tick.ask_qty;
        let obi = if !total_qty.is_zero() {
            (tick.bid_qty - tick.ask_qty) / total_qty
        } else {
            Decimal::ZERO
        };

        match &mut self.position {
            None => {
                // ENTRY LOGIC
                if tick.price < bb_lower && self.last_rsi_value < 30.0 && obi > self.obi_threshold {
                    info!(
                        "⚡ LONG SIGNAL: RSI {:.2} < 30 & OBI {:.2} > {}",
                        self.last_rsi_value, obi, self.obi_threshold
                    );
                    return Ok(Signal::Advice(Side::Buy, tick.price));
                }
            }
            Some(pos) => {
                // 3. TRAILING STOP LOGIC
                if tick.price > pos.highest_price {
                    pos.highest_price = tick.price;
                }

                let trailing_stop_price =
                    pos.highest_price * (Decimal::ONE - self.trailing_callback);

                if tick.price < trailing_stop_price {
                    info!(
                        "🛡️ TRAILING STOP: Price {} < High {} - 0.2%",
                        tick.price, pos.highest_price
                    );
                    return Ok(Signal::Advice(Side::Sell, tick.price));
                }

                let hard_stop = pos.entry_price * Decimal::from_str("0.99").unwrap();
                if tick.price < hard_stop {
                    info!("🛑 HARD STOP LOSS");
                    return Ok(Signal::Advice(Side::Sell, tick.price));
                }
            }
        }

        Ok(Signal::Hold)
    }

    fn update_position(&mut self, position: Option<Position>) {
        self.position = position;
    }
}
